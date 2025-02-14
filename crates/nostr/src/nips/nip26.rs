// Copyright (c) 2022-2023 Yuki Kishimoto
// Distributed under the MIT software license

//! NIP26
//!
//! <https://github.com/nostr-protocol/nips/blob/master/26.md>

use std::fmt;
use std::str::FromStr;

use bitcoin_hashes::sha256::Hash as Sha256Hash;
use bitcoin_hashes::Hash;
use secp256k1::schnorr::Signature;
use secp256k1::{KeyPair, Message, XOnlyPublicKey};
use serde_json::json;

#[cfg(feature = "base")]
use crate::event::Event;
use crate::key::{self, Keys};
use crate::SECP256K1;

const DELEGATION_KEYWORD: &str = "delegation";

/// `NIP26` error
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    /// Key error
    #[error(transparent)]
    Key(#[from] key::Error),
    /// Secp256k1 error
    #[error(transparent)]
    Secp256k1(#[from] secp256k1::Error),
    /// Error passed from NIP-19, Bech32 format, signature format, etc.
    #[error(transparent)]
    Nip19(#[from] crate::nips::nip19::Error),
    /// Invalid condition in conditions string
    #[error("Invalid condition in conditions string")]
    ConditionsParseInvalidCondition,
    /// Invalid condition, cannot parse expected number
    #[error("Invalid condition, cannot parse expected number")]
    ConditionsParseNumeric(#[from] std::num::ParseIntError),
    /// Conditions not satisfied
    #[error("Conditions not satisfied")]
    ConditionsValidation(#[from] ValidationError),
    /// Delegation tag parse error
    #[error("Delegation tag parse error")]
    DelegationTagParse,
}

/// Tag validation errors
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    /// Signature does not match
    #[error("Signature does not match")]
    InvalidSignature,
    /// Event kind does not match
    #[error("Event kind does not match")]
    InvalidKind,
    /// Creation time is earlier than validity period
    #[error("Creation time is earlier than validity period")]
    CreatedTooEarly,
    /// Creation time is later than validity period
    #[error("Creation time is later than validity period")]
    CreatedTooLate,
}

/// Create a NIP-26 delegation tag (including the signature).
/// See also validate_delegation_tag().
pub fn create_delegation_tag(
    delegator_keys: &Keys,
    delegatee_pubkey: XOnlyPublicKey,
    conditions_string: &str,
) -> Result<DelegationTag, Error> {
    DelegationTag::create(delegator_keys, delegatee_pubkey, conditions_string)
}

/// Validate a NIP-26 delegation tag, check signature and conditions.
pub fn validate_delegation_tag(
    delegation_tag: &DelegationTag,
    delegatee_pubkey: XOnlyPublicKey,
    event_properties: &EventProperties,
) -> Result<(), Error> {
    delegation_tag.validate(delegatee_pubkey, event_properties)
}

/// Compile the delegation token, of the form 'nostr:delegation:<pubkey_of_publisher>:<conditions_query_string>'
pub fn delegation_token(delegatee_pk: &XOnlyPublicKey, conditions: &str) -> String {
    format!("nostr:{DELEGATION_KEYWORD}:{delegatee_pk}:{conditions}")
}

/// Sign delegation.
/// See `create_delegation_tag` for more complete functionality.
pub fn sign_delegation(
    delegator_keys: &Keys,
    delegatee_pk: XOnlyPublicKey,
    conditions: String,
) -> Result<Signature, Error> {
    let keypair: &KeyPair = &delegator_keys.key_pair()?;
    let unhashed_token: String = delegation_token(&delegatee_pk, &conditions);
    let hashed_token = Sha256Hash::hash(unhashed_token.as_bytes());
    let message = Message::from_slice(&hashed_token)?;
    Ok(SECP256K1.sign_schnorr(&message, keypair))
}

/// Verify delegation signature
pub fn verify_delegation_signature(
    delegator_public_key: &XOnlyPublicKey,
    signature: &Signature,
    delegatee_public_key: XOnlyPublicKey,
    conditions: String,
) -> Result<(), Error> {
    let unhashed_token: String = delegation_token(&delegatee_public_key, &conditions);
    let hashed_token = Sha256Hash::hash(unhashed_token.as_bytes());
    let message = Message::from_slice(&hashed_token)?;
    SECP256K1.verify_schnorr(signature, &message, delegator_public_key)?;
    Ok(())
}

/// Delegation tag, as defined in NIP-26
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct DelegationTag {
    delegator_pubkey: XOnlyPublicKey,
    conditions: Conditions,
    signature: Signature,
}

impl DelegationTag {
    /// Get delegator public key
    pub fn delegator_pubkey(&self) -> XOnlyPublicKey {
        self.delegator_pubkey
    }

    /// Get conditions
    pub fn conditions(&self) -> Conditions {
        self.conditions.clone()
    }

    /// Get signature
    pub fn signature(&self) -> Signature {
        self.signature
    }

    /// Create a delegation tag (including the signature).
    /// See also validate().
    pub fn create(
        delegator_keys: &Keys,
        delegatee_pubkey: XOnlyPublicKey,
        conditions_string: &str,
    ) -> Result<Self, Error> {
        let signature = sign_delegation(
            delegator_keys,
            delegatee_pubkey,
            conditions_string.to_string(),
        )?;
        let conditions = Conditions::from_str(conditions_string)?;
        Ok(Self {
            delegator_pubkey: delegator_keys.public_key(),
            conditions,
            signature,
        })
    }

    /// Validate a delegation tag, check signature and conditions.
    pub fn validate(
        &self,
        delegatee_pubkey: XOnlyPublicKey,
        event_properties: &EventProperties,
    ) -> Result<(), Error> {
        // verify signature
        if let Err(_e) = verify_delegation_signature(
            &self.delegator_pubkey,
            &self.signature,
            delegatee_pubkey,
            self.conditions.to_string(),
        ) {
            return Err(Error::ConditionsValidation(
                ValidationError::InvalidSignature,
            ));
        }

        // validate conditions
        self.conditions.evaluate(event_properties)?;

        Ok(())
    }

    /// Convert to JSON string.
    pub fn as_json(&self) -> String {
        let tag = json!([
            DELEGATION_KEYWORD,
            self.delegator_pubkey.to_string(),
            self.conditions.to_string(),
            self.signature.to_string(),
        ]);
        tag.to_string()
    }

    /// Parse from a JSON string
    pub fn from_json(s: &str) -> Result<Self, Error> {
        let tag: Vec<String> = serde_json::from_str(s).map_err(|_| Error::DelegationTagParse)?;
        Self::try_from(tag)
    }
}

impl TryFrom<Vec<String>> for DelegationTag {
    type Error = Error;

    fn try_from(tag: Vec<String>) -> Result<Self, Self::Error> {
        if tag.len() != 4 {
            return Err(Error::DelegationTagParse);
        }
        if tag[0] != DELEGATION_KEYWORD {
            return Err(Error::DelegationTagParse);
        }
        Ok(Self {
            delegator_pubkey: XOnlyPublicKey::from_str(&tag[1])?,
            conditions: Conditions::from_str(&tag[2])?,
            signature: Signature::from_str(&tag[3])?,
        })
    }
}

impl fmt::Display for DelegationTag {
    /// Return tag in JSON string format
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_json())
    }
}

impl FromStr for DelegationTag {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_json(s)
    }
}

/// A condition from the delegation conditions.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum Condition {
    /// Event kind, e.g. kind=1
    Kind(u64),
    /// Creation time before, e.g. created_at<1679000000
    CreatedBefore(u64),
    /// Creation time after, e.g. created_at>1676000000
    CreatedAfter(u64),
}

/// Represents properties of an event, relevant for delegation
pub struct EventProperties {
    /// Event kind. For simplicity/flexibility, numeric type is used.
    kind: u64,
    /// Creation time, as unix timestamp
    created_time: u64,
}

impl Condition {
    /// Evaluate whether an event satisfies this condition
    pub(crate) fn evaluate(&self, ep: &EventProperties) -> Result<(), ValidationError> {
        match self {
            Self::Kind(k) => {
                if ep.kind != *k {
                    return Err(ValidationError::InvalidKind);
                }
            }
            Self::CreatedBefore(t) => {
                if ep.created_time >= *t {
                    return Err(ValidationError::CreatedTooLate);
                }
            }
            Self::CreatedAfter(t) => {
                if ep.created_time <= *t {
                    return Err(ValidationError::CreatedTooEarly);
                }
            }
        }
        Ok(())
    }
}

impl ToString for Condition {
    fn to_string(&self) -> String {
        match self {
            Self::Kind(k) => format!("kind={k}"),
            Self::CreatedBefore(t) => format!("created_at<{t}"),
            Self::CreatedAfter(t) => format!("created_at>{t}"),
        }
    }
}

impl FromStr for Condition {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(kind) = s.strip_prefix("kind=") {
            let n = u64::from_str(kind)?;
            return Ok(Self::Kind(n));
        }
        if let Some(created_before) = s.strip_prefix("created_at<") {
            let n = u64::from_str(created_before)?;
            return Ok(Self::CreatedBefore(n));
        }
        if let Some(created_after) = s.strip_prefix("created_at>") {
            let n = u64::from_str(created_after)?;
            return Ok(Self::CreatedAfter(n));
        }
        Err(Error::ConditionsParseInvalidCondition)
    }
}

/// Set of conditions of a delegation.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct Conditions(Vec<Condition>);

impl Default for Conditions {
    fn default() -> Self {
        Self::new()
    }
}

impl Conditions {
    /// New empty [`Conditions`]
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Add [`Condition`]
    pub fn add(&mut self, cond: Condition) {
        self.0.push(cond);
    }

    /// Evaluate whether an event satisfies all these conditions
    fn evaluate(&self, ep: &EventProperties) -> Result<(), ValidationError> {
        for c in &self.0 {
            c.evaluate(ep)?;
        }
        Ok(())
    }

    /// Get [`Vec<Contifion>`]
    pub fn inner(&self) -> Vec<Condition> {
        self.0.clone()
    }
}

impl ToString for Conditions {
    fn to_string(&self) -> String {
        // convert parts, join
        self.0
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>()
            .join("&")
    }
}

impl FromStr for Conditions {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Ok(Self::new());
        }
        let cond = s
            .split('&')
            .map(Condition::from_str)
            .collect::<Result<Vec<Condition>, Self::Err>>()?;
        Ok(Self(cond))
    }
}

impl EventProperties {
    /// Create new with values
    pub fn new(event_kind: u64, created_time: u64) -> Self {
        Self {
            kind: event_kind,
            created_time,
        }
    }

    /// Create from an Event
    #[cfg(feature = "base")]
    pub fn from_event(event: &Event) -> Self {
        Self {
            kind: event.kind.as_u64(),
            created_time: event.created_at.as_u64(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::nips::nip19::ToBech32;
    use crate::prelude::{FromBech32, SecretKey};
    use std::str::FromStr;

    #[test]
    fn test_create_delegation_tag() {
        let delegator_secret_key = SecretKey::from_bech32(
            "nsec1ktekw0hr5evjs0n9nyyquz4sue568snypy2rwk5mpv6hl2hq3vtsk0kpae",
        )
        .unwrap();
        let delegator_keys = Keys::new(delegator_secret_key);
        let delegatee_pubkey = XOnlyPublicKey::from_bech32(
            "npub1h652adkpv4lr8k66cadg8yg0wl5wcc29z4lyw66m3rrwskcl4v6qr82xez",
        )
        .unwrap();
        let conditions = "kind=1&created_at>1676067553&created_at<1678659553".to_string();

        let tag = create_delegation_tag(&delegator_keys, delegatee_pubkey, &conditions).unwrap();

        // verify signature (it's variable)
        let verify_result = verify_delegation_signature(
            &delegator_keys.public_key(),
            &tag.signature(),
            delegatee_pubkey,
            conditions,
        );
        assert!(verify_result.is_ok());

        // signature changes, cannot compare to expected constant, use signature from result
        let expected = format!(
            "[\"delegation\",\"1a459a8a6aa6441d480ba665fb8fb21a4cfe8bcacb7d87300f8046a558a3fce4\",\"kind=1&created_at>1676067553&created_at<1678659553\",\"{}\"]",
            &tag.signature.to_string());
        assert_eq!(tag.to_string(), expected);
    }

    #[test]
    fn test_validate_delegation_tag() {
        let delegator_secret_key = SecretKey::from_bech32(
            "nsec1ktekw0hr5evjs0n9nyyquz4sue568snypy2rwk5mpv6hl2hq3vtsk0kpae",
        )
        .unwrap();
        let delegator_keys = Keys::new(delegator_secret_key);
        let delegatee_pubkey = XOnlyPublicKey::from_bech32(
            "npub1h652adkpv4lr8k66cadg8yg0wl5wcc29z4lyw66m3rrwskcl4v6qr82xez",
        )
        .unwrap();
        let conditions = "kind=1&created_at>1676067553&created_at<1678659553".to_string();

        let tag = create_delegation_tag(&delegator_keys, delegatee_pubkey, &conditions).unwrap();

        assert!(validate_delegation_tag(
            &tag,
            delegatee_pubkey,
            &EventProperties::new(1, 1677000000)
        )
        .is_ok());
    }

    #[test]
    fn test_delegation_tag_parse_and_validate() {
        let tag_str = "[\"delegation\",\"1a459a8a6aa6441d480ba665fb8fb21a4cfe8bcacb7d87300f8046a558a3fce4\",\"kind=1&created_at>1676067553&created_at<1678659553\",\"369aed09c1ad52fceb77ecd6c16f2433eac4a3803fc41c58876a5b60f4f36b9493d5115e5ec5a0ce6c3668ffe5b58d47f2cbc97233833bb7e908f66dbbbd9d36\"]";
        let delegatee_pubkey = XOnlyPublicKey::from_bech32(
            "npub1h652adkpv4lr8k66cadg8yg0wl5wcc29z4lyw66m3rrwskcl4v6qr82xez",
        )
        .unwrap();

        let tag = DelegationTag::from_str(tag_str).unwrap();

        assert!(validate_delegation_tag(
            &tag,
            delegatee_pubkey,
            &EventProperties::new(1, 1677000000)
        )
        .is_ok());

        // additional test: verify a value from inside the tag
        assert_eq!(
            tag.conditions().to_string(),
            "kind=1&created_at>1676067553&created_at<1678659553"
        );

        // additional test: try validation with invalid values, invalid event kind
        match validate_delegation_tag(&tag, delegatee_pubkey, &EventProperties::new(5, 1677000000))
            .err()
            .unwrap()
        {
            Error::ConditionsValidation(e) => assert_eq!(e, ValidationError::InvalidKind),
            _ => panic!("Expected ConditionsValidation"),
        };
    }

    #[test]
    fn test_sign_delegation_verify_delegation_signature() {
        let delegator_secret_key =
            SecretKey::from_str("ee35e8bb71131c02c1d7e73231daa48e9953d329a4b701f7133c8f46dd21139c")
                .unwrap();
        let delegator_keys = Keys::new(delegator_secret_key);
        let delegatee_public_key = XOnlyPublicKey::from_bech32(
            "npub1gae33na4gfaeelrx48arwc2sc8wmccs3tt38emmjg9ltjktfzwtqtl4l6u",
        )
        .unwrap();
        let conditions = "kind=1&created_at>1674834236&created_at<1677426236".to_string();

        let signature =
            sign_delegation(&delegator_keys, delegatee_public_key, conditions.clone()).unwrap();

        // signature is changing, validate by verify method
        let verify_result = verify_delegation_signature(
            &delegator_keys.public_key(),
            &signature,
            delegatee_public_key,
            conditions,
        );
        assert!(verify_result.is_ok());
    }

    #[test]
    fn test_sign_delegation_verify_lowlevel() {
        let delegator_secret_key =
            SecretKey::from_str("ee35e8bb71131c02c1d7e73231daa48e9953d329a4b701f7133c8f46dd21139c")
                .unwrap();
        let delegator_keys = Keys::new(delegator_secret_key);
        let delegatee_public_key = XOnlyPublicKey::from_bech32(
            "npub1gae33na4gfaeelrx48arwc2sc8wmccs3tt38emmjg9ltjktfzwtqtl4l6u",
        )
        .unwrap();
        let conditions = "kind=1&created_at>1674834236&created_at<1677426236";

        let signature = sign_delegation(
            &delegator_keys,
            delegatee_public_key,
            conditions.to_string(),
        )
        .unwrap();

        // signature is changing, validate by lowlevel verify
        let unhashed_token: String =
            format!("nostr:delegation:{delegatee_public_key}:{conditions}");
        let hashed_token = Sha256Hash::hash(unhashed_token.as_bytes());
        let message = Message::from_slice(&hashed_token).unwrap();

        let verify_result =
            SECP256K1.verify_schnorr(&signature, &message, &delegator_keys.public_key());
        assert!(verify_result.is_ok());
    }

    #[test]
    fn test_verify_delegation_signature() {
        let delegator_secret_key =
            SecretKey::from_str("ee35e8bb71131c02c1d7e73231daa48e9953d329a4b701f7133c8f46dd21139c")
                .unwrap();
        let delegator_keys = Keys::new(delegator_secret_key);
        // use one concrete signature
        let signature = Signature::from_str("f9f00fcf8480686d9da6dfde1187d4ba19c54f6ace4c73361a14db429c4b96eb30b29283d6ea1f06ba9e18e06e408244c689039ddadbacffc56060f3da5b04b8").unwrap();
        let delegatee_pk = XOnlyPublicKey::from_bech32(
            "npub1gae33na4gfaeelrx48arwc2sc8wmccs3tt38emmjg9ltjktfzwtqtl4l6u",
        )
        .unwrap();
        let conditions = "kind=1&created_at>1674834236&created_at<1677426236".to_string();

        let verify_result = verify_delegation_signature(
            &delegator_keys.public_key(),
            &signature,
            delegatee_pk,
            conditions,
        );
        assert!(verify_result.is_ok());
    }

    #[test]
    fn test_delegation_token() {
        let delegatee_pk = XOnlyPublicKey::from_bech32(
            "npub1gae33na4gfaeelrx48arwc2sc8wmccs3tt38emmjg9ltjktfzwtqtl4l6u",
        )
        .unwrap();
        let conditions = "kind=1&created_at>1674834236&created_at<1677426236";
        let unhashed_token: String = delegation_token(&delegatee_pk, &conditions);
        assert_eq!(
            unhashed_token,
            "nostr:delegation:477318cfb5427b9cfc66a9fa376150c1ddbc62115ae27cef72417eb959691396:kind=1&created_at>1674834236&created_at<1677426236"
        );
    }

    #[test]
    fn test_delegation_tag_to_json() {
        let delegator_sk = SecretKey::from_bech32(
            "nsec1ktekw0hr5evjs0n9nyyquz4sue568snypy2rwk5mpv6hl2hq3vtsk0kpae",
        )
        .unwrap();
        let delegator_pubkey = Keys::new(delegator_sk).public_key();
        let conditions = Conditions::from_str("kind=1&created_at<1678659553").unwrap();
        let signature = Signature::from_str("435091ab4c4a11e594b1a05e0fa6c2f6e3b6eaa87c53f2981a3d6980858c40fdcaffde9a4c461f352a109402a4278ff4dbf90f9ebd05f96dac5ae36a6364a976").unwrap();
        let d = DelegationTag {
            delegator_pubkey,
            conditions,
            signature,
        };
        let tag = d.as_json();
        assert_eq!(tag, "[\"delegation\",\"1a459a8a6aa6441d480ba665fb8fb21a4cfe8bcacb7d87300f8046a558a3fce4\",\"kind=1&created_at<1678659553\",\"435091ab4c4a11e594b1a05e0fa6c2f6e3b6eaa87c53f2981a3d6980858c40fdcaffde9a4c461f352a109402a4278ff4dbf90f9ebd05f96dac5ae36a6364a976\"]");
    }

    #[test]
    fn test_delegation_tag_from_str() {
        let tag_str = "[\"delegation\",\"1a459a8a6aa6441d480ba665fb8fb21a4cfe8bcacb7d87300f8046a558a3fce4\",\"kind=1&created_at>1676067553&created_at<1678659553\",\"369aed09c1ad52fceb77ecd6c16f2433eac4a3803fc41c58876a5b60f4f36b9493d5115e5ec5a0ce6c3668ffe5b58d47f2cbc97233833bb7e908f66dbbbd9d36\"]";

        let tag = DelegationTag::from_str(tag_str).unwrap();

        assert_eq!(tag.to_string(), tag_str);
        assert_eq!(
            tag.conditions().to_string(),
            "kind=1&created_at>1676067553&created_at<1678659553"
        );
        assert_eq!(
            tag.delegator_pubkey().to_bech32().unwrap(),
            "npub1rfze4zn25ezp6jqt5ejlhrajrfx0az72ed7cwvq0spr22k9rlnjq93lmd4"
        );
    }

    #[test]
    fn test_validate_delegation_tag_negative() {
        let delegator_secret_key = SecretKey::from_bech32(
            "nsec1ktekw0hr5evjs0n9nyyquz4sue568snypy2rwk5mpv6hl2hq3vtsk0kpae",
        )
        .unwrap();
        let delegator_keys = Keys::new(delegator_secret_key);
        let delegatee_pubkey = XOnlyPublicKey::from_bech32(
            "npub1h652adkpv4lr8k66cadg8yg0wl5wcc29z4lyw66m3rrwskcl4v6qr82xez",
        )
        .unwrap();
        let conditions = "kind=1&created_at>1676067553&created_at<1678659553".to_string();

        let tag = create_delegation_tag(&delegator_keys, delegatee_pubkey, &conditions).unwrap();

        // positive
        assert!(validate_delegation_tag(
            &tag,
            delegatee_pubkey,
            &EventProperties::new(1, 1677000000)
        )
        .is_ok());

        // signature verification fails if wrong delegatee key is given
        let wrong_pubkey = XOnlyPublicKey::from_bech32(
            "npub1zju3cgxq9p6f2c2jzrhhwuse94p7efkj5dp59eerh53hqd08j4dszevd7s",
        )
        .unwrap();
        // Note: Error cannot be tested simply  using equality
        match validate_delegation_tag(&tag, wrong_pubkey, &EventProperties::new(1, 1677000000))
            .err()
            .unwrap()
        {
            Error::ConditionsValidation(e) => assert_eq!(e, ValidationError::InvalidSignature),
            _ => panic!("Expected ConditionsValidation"),
        }

        // wrong event kind
        match validate_delegation_tag(&tag, delegatee_pubkey, &EventProperties::new(9, 1677000000))
            .err()
            .unwrap()
        {
            Error::ConditionsValidation(e) => assert_eq!(e, ValidationError::InvalidKind),
            _ => panic!("Expected ConditionsValidation"),
        };

        // wrong creation time
        match validate_delegation_tag(&tag, delegatee_pubkey, &EventProperties::new(1, 1679000000))
            .err()
            .unwrap()
        {
            Error::ConditionsValidation(e) => assert_eq!(e, ValidationError::CreatedTooLate),
            _ => panic!("Expected ConditionsValidation"),
        };
    }

    #[test]
    fn test_conditions_to_string() {
        let mut c = Conditions::new();
        c.add(Condition::Kind(1));
        assert_eq!(c.to_string(), "kind=1");
        c.add(Condition::CreatedAfter(1674834236));
        c.add(Condition::CreatedBefore(1677426236));
        assert_eq!(
            c.to_string(),
            "kind=1&created_at>1674834236&created_at<1677426236"
        );
    }

    #[test]
    fn test_conditions_parse() {
        let c = Conditions::from_str("kind=1&created_at>1674834236&created_at<1677426236").unwrap();
        assert_eq!(
            c.to_string(),
            "kind=1&created_at>1674834236&created_at<1677426236"
        );

        // special: empty string
        let c_empty = Conditions::from_str("").unwrap();
        assert_eq!(c_empty.to_string(), "");

        // one condition
        let c_one = Conditions::from_str("created_at<10000").unwrap();
        assert_eq!(c_one.to_string(), "created_at<10000");
    }

    #[test]
    fn test_conditions_parse_negative() {
        match Conditions::from_str("__invalid_condition__&kind=1")
            .err()
            .unwrap()
        {
            Error::ConditionsParseInvalidCondition => {}
            _ => panic!("Exepected ConditionsParseInvalidCondition"),
        }
        match Conditions::from_str("kind=__invalid_number__")
            .err()
            .unwrap()
        {
            Error::ConditionsParseNumeric(_) => {}
            _ => panic!("Exepected ConditionsParseNumeric"),
        }
    }

    #[test]
    fn test_conditions_evaluate() {
        let c_kind = Conditions::from_str("kind=3").unwrap();
        assert!(c_kind.evaluate(&EventProperties::new(3, 0)).is_ok());
        assert_eq!(
            c_kind.evaluate(&EventProperties::new(5, 0)).err().unwrap(),
            ValidationError::InvalidKind
        );

        let c_impossible = Conditions::from_str("kind=3&kind=4").unwrap();
        assert_eq!(
            c_impossible
                .evaluate(&EventProperties::new(3, 0))
                .err()
                .unwrap(),
            ValidationError::InvalidKind
        );

        let c_before = Conditions::from_str("created_at<1000").unwrap();
        assert!(c_before.evaluate(&EventProperties::new(3, 500)).is_ok());
        assert_eq!(
            c_before
                .evaluate(&EventProperties::new(3, 2000))
                .err()
                .unwrap(),
            ValidationError::CreatedTooLate
        );

        let c_after = Conditions::from_str("created_at>1000").unwrap();
        assert!(c_after.evaluate(&EventProperties::new(3, 2000)).is_ok());
        assert_eq!(
            c_after
                .evaluate(&EventProperties::new(3, 500))
                .err()
                .unwrap(),
            ValidationError::CreatedTooEarly
        );

        let c_complex =
            Conditions::from_str("kind=1&created_at>1676067553&created_at<1678659553").unwrap();
        assert!(c_complex
            .evaluate(&EventProperties::new(1, 1677000000))
            .is_ok());
        //assert_eq!(c_complex.evaluate(&EventProperties{ kind: 1, created_time: 1677000000}).err().unwrap(), ValidationError::InvalidKind);
        assert_eq!(
            c_complex
                .evaluate(&EventProperties::new(5, 1677000000))
                .err()
                .unwrap(),
            ValidationError::InvalidKind
        );
        assert_eq!(
            c_complex
                .evaluate(&EventProperties::new(1, 1674000000))
                .err()
                .unwrap(),
            ValidationError::CreatedTooEarly
        );
        assert_eq!(
            c_complex
                .evaluate(&EventProperties::new(1, 1699000000))
                .err()
                .unwrap(),
            ValidationError::CreatedTooLate
        );
    }
}
