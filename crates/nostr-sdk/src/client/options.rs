// Copyright (c) 2022-2023 Yuki Kishimoto
// Distributed under the MIT software license

#[cfg(feature = "nip13")]
use std::sync::atomic::AtomicU8;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Options
#[derive(Debug, Clone)]
pub struct Options {
    /// Wait for connection
    wait_for_connection: Arc<AtomicBool>,
    /// Wait for the msg to be sent
    wait_for_send: Arc<AtomicBool>,
    /// POW difficulty (for all events)
    #[cfg(feature = "nip13")]
    difficulty: Arc<AtomicU8>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            wait_for_connection: Arc::new(AtomicBool::new(false)),
            wait_for_send: Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "nip13")]
            difficulty: Arc::new(AtomicU8::new(0)),
        }
    }
}

impl Options {
    /// Create new (default) [`Options`]
    pub fn new() -> Self {
        Self::default()
    }

    /// If set to `true`, `Client` wait that `Relay` try at least one time to enstablish a connection before continue.
    pub fn wait_for_connection(self, wait: bool) -> Self {
        Self {
            wait_for_connection: Arc::new(AtomicBool::new(wait)),
            ..self
        }
    }

    pub(crate) fn get_wait_for_connection(&self) -> bool {
        self.wait_for_connection.load(Ordering::SeqCst)
    }

    /// If set to `true`, `Client` wait that an event is sent before continue.
    pub fn wait_for_send(self, wait: bool) -> Self {
        Self {
            wait_for_send: Arc::new(AtomicBool::new(wait)),
            ..self
        }
    }

    pub(crate) fn get_wait_for_send(&self) -> bool {
        self.wait_for_send.load(Ordering::SeqCst)
    }

    /// Set default POW diffficulty for `Event`
    #[cfg(feature = "nip13")]
    pub fn difficulty(self, difficulty: u8) -> Self {
        Self {
            difficulty: Arc::new(AtomicU8::new(difficulty)),
            ..self
        }
    }

    #[cfg(feature = "nip13")]
    pub(crate) fn get_difficulty(&self) -> u8 {
        self.difficulty.load(Ordering::SeqCst)
    }

    #[cfg(feature = "nip13")]
    pub(crate) fn update_difficulty(&self, difficulty: u8) {
        let _ = self
            .difficulty
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |_| Some(difficulty));
    }
}
