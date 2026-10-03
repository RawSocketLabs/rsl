//! Owned TCP BIND stages with bounded control I/O and listener acceptance.
// --- Standard library ---
use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    time::{Duration, Instant},
};

// --- Internal modules ---
use super::{AwaitingPeer, BindRequest, bind_listener::BindListener, exchange_bind};
use crate::{
    Stream,
    error::Error,
    io::blocking::{Deadline, deadline, remaining, unbounded},
    server::policy::{Authentication, ServerAuth},
    v5::{Endpoint, ReplyCode},
};

/// An authenticated TCP BIND request under its original handshake deadline.
/// Authorize the request before creating the listener passed to `listen`.
pub struct TcpBindRequest {
    request: BindRequest<Deadline>,
}

/// A listening TCP BIND exchange with a running absolute peer-wait deadline.
/// Owns and closes both listener and control socket on failure or abandonment.
pub struct TcpAwaitingPeer {
    waiting: AwaitingPeer<Deadline>,
    listener: BindListener,
}

impl TcpBindRequest {
    /// Original expected-peer hint for listener-creation policy.
    #[must_use]
    pub fn destination(&self) -> &Endpoint {
        self.request.destination()
    }
    /// Authentication established by the exchange.
    #[must_use]
    pub fn authentication(&self) -> Authentication {
        self.request.authentication()
    }

    /// Register an authorized listener and send the first success reply.
    /// `advertised` is supplied explicitly for wildcard binds or NAT; the caller
    /// is responsible for making it reachable. The peer budget begins after this
    /// reply is flushed, not when `accept` is later called.
    ///
    /// # Errors
    /// Setup failures send a bounded failure reply. First-success write failures
    /// close both sockets without attempting a replacement reply.
    pub fn listen(
        self,
        listener: TcpListener,
        advertised: SocketAddr,
        peer_timeout: Duration,
    ) -> Result<TcpAwaitingPeer, Error> {
        let prepared = (|| {
            remaining(self.request.get_ref().deadline)?;
            deadline(peer_timeout)?;
            Ok::<_, Error>(BindListener::new(listener)?)
        })();
        let listener = match prepared {
            Ok(listener) => listener,
            Err(error) => return fail_request(self.request, error),
        };
        let mut waiting = self.request.send_bound(advertised)?;
        if let Err(error) = remaining(waiting.get_ref().deadline) {
            return fail_waiting(waiting, error.into());
        }
        waiting.get_mut().deadline = deadline(peer_timeout)?;
        Ok(TcpAwaitingPeer { waiting, listener })
    }

    /// Reject listener creation and close the owned session.
    ///
    /// # Errors
    /// Rejects success codes or a terminal reply-write failure.
    pub fn send_failure(mut self, code: ReplyCode) -> Result<(), Error> {
        self.request.get_mut().deadline = deadline(Duration::from_secs(1))?;
        self.request.send_failure(code)
    }
}
impl TcpAwaitingPeer {
    /// Expected-peer hint for the incoming peer policy.
    pub fn destination(&self) -> &Endpoint {
        self.waiting.destination()
    }
    /// Authentication established before listening.
    pub fn authentication(&self) -> Authentication {
        self.waiting.authentication()
    }
    /// Absolute expiry shared by accept, peer policy, and the second success reply.
    #[must_use]
    pub fn deadline(&self) -> Instant {
        self.waiting.get_ref().deadline
    }

    /// Accept the anticipated peer, require explicit policy approval, and send the second reply.
    /// A peer that policy rejects is closed and acceptance continues, so a stray connection to
    /// the advertised port cannot end the BIND; only expiry fails it. Policy must be fast,
    /// nonblocking, and non-panicking; each return is checked against the same deadline.
    /// Caller code cannot be preempted. Approval cannot reset the budget.
    /// Accepted sockets are blocking.
    ///
    /// # Errors
    /// Acceptance failures or expiry close all owned sockets after a best-effort
    /// failure reply bounded to one second. Success-write failures are terminal
    /// and never followed by a second attempt or failure reply.
    pub fn accept(
        mut self,
        mut authorize: impl FnMut(SocketAddr) -> bool,
    ) -> Result<(Stream<TcpStream>, TcpStream), Error> {
        let until = self.deadline();
        let accepted = (|| {
            loop {
                let (socket, peer) = self.listener.accept(until)?;
                let allowed = authorize(peer);
                remaining(until)?;
                if allowed {
                    return Ok::<_, Error>((socket, peer));
                }
                // Close the rejected peer and keep waiting for the anticipated one.
                drop(socket);
            }
        })();
        let (socket, peer) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => return fail_waiting(self.waiting, error),
        };
        // Authorization precedes success; any error after writing begins only closes sockets.
        let stream = self.waiting.send_success(peer, |_| true)?;
        Ok((unbounded(stream)?, socket))
    }

    /// Abort waiting and send a failing second reply with a one-second write budget.
    ///
    /// # Errors
    /// Rejects a success code or reports a terminal write/flush failure.
    pub fn send_failure(mut self, code: ReplyCode) -> Result<(), Error> {
        self.waiting.get_mut().deadline = deadline(Duration::from_secs(1))?;
        self.waiting.send_failure(code)
    }
}

/// Authenticate and receive BIND under one absolute TCP handshake deadline.
/// The same budget remains active until `listen` has flushed the first reply.
///
/// ```no_run
/// use socks::{server::policy::ServerAuth, v5::{Endpoint, ReplyCode, server::blocking::exchange_bind_tcp}};
/// use std::{net::{TcpListener, TcpStream}, time::Duration};
/// # fn handle(control: TcpStream) -> Result<(), socks::error::Error> {
/// let request = exchange_bind_tcp(control, &ServerAuth::no_authentication(), Duration::from_secs(5))?;
///
/// // Example policy: only a numeric loopback peer may request a listener.
/// let allowed = matches!(request.destination(), Endpoint::Ipv4 { address, .. } if address.is_loopback());
/// if !allowed { return request.send_failure(ReplyCode::ConnectionNotAllowed); }
///
/// // Advertise the listener only after the request passes policy.
/// let listener = TcpListener::bind("127.0.0.1:0")?;
/// let advertised = listener.local_addr()?;
/// let waiting = request.listen(listener, advertised, Duration::from_secs(30))?;
///
/// // Authorize the actual incoming peer, independently of its claimed hint.
/// let (client, peer) = waiting.accept(|peer| peer.ip().is_loopback())?;
///
/// // Relay client <-> peer here; retain the client's Stream wrapper.
/// # let _ = (client, peer);
/// # Ok(()) }
/// ```
///
/// # Errors
/// Invalid timeouts and terminal protocol/I/O failures close the owned socket.
pub fn exchange_bind_tcp(
    stream: TcpStream,
    auth: &ServerAuth,
    handshake_timeout: Duration,
) -> Result<TcpBindRequest, Error> {
    let until = deadline(handshake_timeout)?;
    let request = exchange_bind(
        Deadline {
            stream,
            deadline: until,
        },
        auth,
    )?;
    remaining(until)?;
    Ok(TcpBindRequest { request })
}

/// Preserve the triggering error when a first failure reply succeeds.
fn fail_request<T>(mut request: BindRequest<Deadline>, error: Error) -> Result<T, Error> {
    request.get_mut().deadline = deadline(Duration::from_secs(1))?;
    request.send_failure(error.reply_code())?;
    Err(error)
}

/// Preserve the triggering error when a second failure reply succeeds.
fn fail_waiting<T>(mut waiting: AwaitingPeer<Deadline>, error: Error) -> Result<T, Error> {
    waiting.get_mut().deadline = deadline(Duration::from_secs(1))?;
    waiting.send_failure(error.reply_code())?;
    Err(error)
}
