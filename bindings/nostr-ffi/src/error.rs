// Copyright (c) 2022-2023 Yuki Kishimoto
// Distributed under the MIT software license

use std::fmt;

pub type Result<T, E = NostrError> = std::result::Result<T, E>;

#[derive(Debug)]
pub enum NostrError {
    Generic { err: String },
}

impl fmt::Display for NostrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generic { err } => write!(f, "{err}"),
        }
    }
}

impl From<nostr::key::Error> for NostrError {
    fn from(e: nostr::key::Error) -> NostrError {
        Self::Generic { err: e.to_string() }
    }
}

impl From<nostr::event::Error> for NostrError {
    fn from(e: nostr::event::Error) -> NostrError {
        Self::Generic { err: e.to_string() }
    }
}

impl From<nostr::event::builder::Error> for NostrError {
    fn from(e: nostr::event::builder::Error) -> NostrError {
        Self::Generic { err: e.to_string() }
    }
}

impl From<nostr::event::tag::Error> for NostrError {
    fn from(e: nostr::event::tag::Error) -> NostrError {
        Self::Generic { err: e.to_string() }
    }
}

impl From<nostr::nips::nip19::Error> for NostrError {
    fn from(e: nostr::nips::nip19::Error) -> NostrError {
        Self::Generic { err: e.to_string() }
    }
}

impl From<nostr::secp256k1::Error> for NostrError {
    fn from(e: nostr::secp256k1::Error) -> NostrError {
        Self::Generic { err: e.to_string() }
    }
}

impl From<nostr::url::ParseError> for NostrError {
    fn from(e: nostr::url::ParseError) -> NostrError {
        Self::Generic { err: e.to_string() }
    }
}

impl From<nostr::hashes::hex::Error> for NostrError {
    fn from(e: nostr::hashes::hex::Error) -> NostrError {
        Self::Generic { err: e.to_string() }
    }
}

impl From<nostr::event::id::Error> for NostrError {
    fn from(e: nostr::event::id::Error) -> NostrError {
        Self::Generic { err: e.to_string() }
    }
}
