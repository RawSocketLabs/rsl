//! Generic two-reply BIND client exchange.
// --- Standard library ---
use std::io::{Read, Write};

// --- Internal modules ---
use super::exchange::exchange;
use crate::{
    Stream,
    error::Error,
    v5::{Command, Endpoint, Reply, auth::ClientAuth},
};

/// A successful BIND listener reply, still awaiting the connecting-peer reply.
/// This stage does not implement application I/O. Generic transports require
/// caller-managed deadlines; use `bind_tcp` for bounded TCP operation.
pub struct Binding<S> {
    pub(super) stream: Stream<S>,
    pub(super) bound: Endpoint,
}

impl<S> Binding<S> {
    /// The proxy's advertised listener, to communicate over the application's control channel.
    #[must_use]
    pub fn bound(&self) -> &Endpoint {
        &self.bound
    }

    /// Inspect or configure the transport; direct I/O bypasses retained protocol bytes.
    #[must_use]
    pub fn get_ref(&self) -> &S {
        self.stream.get_ref()
    }

    /// Configure caller-managed deadlines; do not read or write protocol bytes through this borrow.
    pub fn get_mut(&mut self) -> &mut S {
        self.stream.get_mut()
    }
}

impl<S: Read + Write> Binding<S> {
    /// Consume the second reply and return the lossless stream and connecting peer.
    ///
    /// # Errors
    /// A failed, malformed, or truncated reply is terminal. Dropping an owned
    /// transport closes it; a caller supplying a borrow must close it on failure.
    pub fn wait_for_peer(mut self) -> Result<(Stream<S>, Endpoint), Error> {
        let reply: Reply = self.stream.read_message()?;
        reply.ensure_success()?;
        Ok((self.stream, reply.bound))
    }
}

/// Authenticate, request BIND, and receive the first (listener) reply.
///
/// `expected_peer` is a hint for the proxy's policy, not a local listener address.
/// Zero ports are preserved; no wildcard, DNS matching, or peer authorization
/// policy is inferred. Generic transports need caller-enforced phase deadlines.
///
/// # Errors
/// Invalid local construction fails before I/O; negotiation/reply errors are terminal.
pub fn bind<S: Read + Write>(
    stream: S,
    expected_peer: Endpoint,
    auth: ClientAuth<'_>,
) -> Result<Binding<S>, Error> {
    expected_peer.validate_structure()?;
    let credentials = auth.credentials()?;
    let (stream, bound) = exchange(
        stream,
        expected_peer,
        Command::Bind,
        auth.method(),
        credentials,
    )?;
    Ok(Binding { stream, bound })
}
