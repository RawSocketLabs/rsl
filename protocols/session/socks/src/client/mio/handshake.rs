// --- Standard library ---
use std::io::{Read, Write};

// --- Workspace dependencies ---
use mio::Interest;

// --- Internal modules ---
use crate::v5::client::mio::Client as V5Handshake;
use crate::{client::Connection, error::Error};

/// A caller-driven handshake that retains partial I/O and unread application bytes.
/// The caller owns its poll, registration, and deadline. Dropping this stage drops
/// its transport; callers passing a borrowed transport remain responsible for closing it.
pub struct Handshake<S> {
    inner: V5Handshake<S>,
}

impl<S: Read + Write> Handshake<S> {
    /// Wrap the selected version's handshake without adding scheduling or buffering.
    pub(super) fn new(inner: V5Handshake<S>) -> Self {
        Self { inner }
    }

    /// Required readiness while incomplete; absent after completion or terminal failure.
    #[must_use]
    pub fn interest(&self) -> Option<Interest> {
        self.inner.interest()
    }

    /// Whether fairness requires another advance without waiting for a readiness edge.
    #[must_use]
    pub fn needs_advance(&self) -> bool {
        self.inner.needs_advance()
    }

    /// Borrow the transport for registration/configuration, never application I/O.
    ///
    /// # Errors
    /// Reports invalid state after failure or handoff.
    pub fn transport_mut(&mut self) -> Result<&mut S, Error> {
        self.inner.transport_mut()
    }

    /// Advance until blocked, yielded, or complete; true permits handoff.
    /// Check caller-owned deadlines before and after this call.
    ///
    /// # Errors
    /// Errors are terminal and drop owned transports; `WouldBlock` is an incomplete result.
    pub fn advance(&mut self) -> Result<bool, Error> {
        self.inner.advance()
    }

    /// Take the established connection once, preserving input and its original poll association.
    /// Returns None before completion, after failure, or after an earlier handoff.
    /// Drain the returned connection until `WouldBlock` before waiting for readiness:
    /// prefetched bytes, and kernel bytes from a readiness event the handshake consumed,
    /// raise no new edge-triggered event.
    pub fn take_connection(&mut self) -> Option<Connection<S>> {
        self.inner
            .take_stream()
            .map(|(stream, bound)| Connection::new(stream, bound.into()))
    }
}
