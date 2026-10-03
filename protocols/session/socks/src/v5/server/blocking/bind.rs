//! Caller-managed BIND listener and connecting-peer replies.
// --- Standard library ---
use std::{
    io::{Read, Write},
    net::SocketAddr,
};

// --- Internal modules ---
use super::exchange::{AuthenticatedRequest, exchange};
use crate::{
    Stream,
    error::Error,
    server::policy::{Authentication, ServerAuth},
    v5::{Command, Endpoint, Reply, ReplyCode},
};

/// An authenticated BIND request awaiting listener-creation authorization.
/// The caller must authorize the original hint and authentication before creating
/// a listener. No DNS matching or wildcard policy is implied.
pub struct BindRequest<S> {
    pub(super) request: AuthenticatedRequest<S>,
}

/// A sent BIND listener reply, awaiting one explicitly authorized incoming peer.
/// This owns the same buffered stream as the first stage and cannot expose it as
/// an application stream before the second success reply.
pub struct AwaitingPeer<S> {
    pub(super) request: AuthenticatedRequest<S>,
}

impl<S> BindRequest<S> {
    /// Original expected-peer hint, including unresolved domain bytes.
    #[must_use]
    pub fn destination(&self) -> &Endpoint {
        &self.request.destination
    }
    /// Authentication actually completed before this request.
    #[must_use]
    pub fn authentication(&self) -> Authentication {
        self.request.authentication
    }
    /// Configure or inspect the underlying transport without bypassing buffered I/O.
    #[must_use]
    pub fn get_ref(&self) -> &S {
        self.request.stream.get_ref()
    }
    /// Configure caller-managed deadlines; direct protocol I/O bypasses this stage.
    pub fn get_mut(&mut self) -> &mut S {
        self.request.stream.get_mut()
    }
}
impl<S> AwaitingPeer<S> {
    /// Original expected-peer hint for the caller's inbound-peer policy.
    pub fn destination(&self) -> &Endpoint {
        &self.request.destination
    }
    /// Authentication actually completed before BIND.
    pub fn authentication(&self) -> Authentication {
        self.request.authentication
    }
    /// Inspect the underlying transport, without reading retained protocol bytes.
    pub fn get_ref(&self) -> &S {
        self.request.stream.get_ref()
    }
    /// Configure the second-phase deadline; do not perform direct protocol I/O.
    pub fn get_mut(&mut self) -> &mut S {
        self.request.stream.get_mut()
    }
}
impl<S: Read + Write> BindRequest<S> {
    /// Send the first reply after creating and authorizing a listener.
    /// The caller supplies its advertised address, which may differ under NAT.
    ///
    /// # Errors
    /// A write/flush failure is terminal; never attempt a replacement reply.
    pub fn send_bound(mut self, bound: SocketAddr) -> Result<AwaitingPeer<S>, Error> {
        self.request
            .stream
            .write_message(&Reply::success(bound.into())?)?;
        Ok(AwaitingPeer {
            request: self.request,
        })
    }
    /// Send a failing first reply and consume the session.
    ///
    /// # Errors
    /// Rejects success codes or reports a terminal write/flush failure.
    pub fn send_failure(mut self, code: ReplyCode) -> Result<(), Error> {
        self.request.stream.write_message(&Reply::failure(code)?)
    }
}
impl<S: Read + Write> AwaitingPeer<S> {
    /// Authorize an accepted numeric peer before sending the second success reply.
    /// The callback must be fast and nonblocking. The caller must obtain `peer`
    /// from the accepted socket; approval alone is not proof of a connection.
    ///
    /// # Errors
    /// Denial sends a failing second reply and returns `PermissionDenied`.
    /// Write/flush failure is terminal, with no fallback reply.
    pub fn send_success(
        mut self,
        peer: SocketAddr,
        authorize: impl FnOnce(SocketAddr) -> bool,
    ) -> Result<Stream<S>, Error> {
        if !authorize(peer) {
            self.send_failure(ReplyCode::ConnectionNotAllowed)?;
            return Err(Error::PermissionDenied);
        }
        self.request
            .stream
            .write_message(&Reply::success(peer.into())?)?;
        Ok(self.request.stream)
    }
    /// Send a failing second reply and consume the session.
    ///
    /// # Errors
    /// Rejects success codes or reports terminal transport failure.
    pub fn send_failure(mut self, code: ReplyCode) -> Result<(), Error> {
        self.request.stream.write_message(&Reply::failure(code)?)
    }
}

/// Authenticate and receive only BIND; listener creation and policy remain caller-owned.
/// Generic transports require caller-enforced handshake, accept, and reply deadlines.
/// Owned transports close on failure; callers supplying borrows must close them.
///
/// # Errors
/// Rejects other commands and malformed requests without accepting a peer.
pub fn exchange_bind<S: Read + Write>(
    stream: S,
    auth: &ServerAuth,
) -> Result<BindRequest<S>, Error> {
    Ok(BindRequest {
        request: exchange(stream, auth, Command::Bind)?,
    })
}
