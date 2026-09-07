use std::net::{SocketAddr, TcpListener, ToSocketAddrs};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::auth::{Authenticator, NoAuth};
use crate::error::Result;
use crate::server::connection::handle_client;
use crate::server::pool::Semaphore;

/// Default deadline for a client to complete method negotiation,
/// authentication, and the command request before the server drops it.
pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Default deadline for a BIND request to receive its inbound peer connection
/// before the server gives up and frees the handler.
pub const DEFAULT_BIND_TIMEOUT: Duration = Duration::from_secs(120);

/// Default cap on concurrent connection handlers in [`Server::serve`].
pub const DEFAULT_MAX_CONNECTIONS: usize = 1024;

/// A SOCKS5 proxy server (RFC 1928).
///
/// [`bind`](Self::bind) it to an address, optionally configure authentication
/// and the resource-exhaustion guards, then drive it with
/// [`serve`](Self::serve) (loop, one thread per client) or
/// [`accept`](Self::accept) (one client, current thread). By default it accepts
/// unauthenticated clients; pass [`Authenticator`]s to
/// [`with_authenticators`](Self::with_authenticators) to require credentials.
///
/// The builder-style setters consume and return `self`, so they chain:
///
/// ```no_run
/// use std::time::Duration;
/// use socks::auth::UserPassAuthenticator;
/// use socks::server::Server;
///
/// # fn main() -> Result<(), socks::error::SocksError> {
/// Server::bind("127.0.0.1:1080")?
///     .with_authenticators(vec![Box::new(UserPassAuthenticator::new(
///         |user, pass| user == b"admin" && pass == b"hunter2",
///     ))])
///     .with_handshake_timeout(Some(Duration::from_secs(10)))
///     .with_max_connections(256)
///     .serve()?;
/// # Ok(())
/// # }
/// ```
pub struct Server {
    listener: TcpListener,
    authenticators: Arc<Vec<Box<dyn Authenticator>>>,
    handshake_timeout: Option<Duration>,
    bind_timeout: Option<Duration>,
    max_connections: usize,
}

impl Server {
    /// Binds a server that accepts unauthenticated clients.
    ///
    /// # Errors
    /// Returns an error if the listener cannot be bound.
    pub fn bind(addr: impl ToSocketAddrs) -> Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(addr)?,
            authenticators: Arc::new(vec![Box::new(NoAuth)]),
            handshake_timeout: Some(DEFAULT_HANDSHAKE_TIMEOUT),
            bind_timeout: Some(DEFAULT_BIND_TIMEOUT),
            max_connections: DEFAULT_MAX_CONNECTIONS,
        })
    }

    /// Replaces the accepted authentication methods, in preference order.
    pub fn with_authenticators(mut self, authenticators: Vec<Box<dyn Authenticator>>) -> Self {
        self.authenticators = Arc::new(authenticators);
        self
    }

    /// Sets the deadline for completing the handshake (negotiation, auth, and
    /// request). `None` disables it. Guards against clients that connect and
    /// then stall, holding a handler open indefinitely. The timeout applies
    /// only to the handshake; an established relay is not time-limited.
    pub fn with_handshake_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    /// Sets the deadline for a BIND request to receive its inbound peer
    /// connection. `None` waits indefinitely. Guards against BIND requests
    /// whose peer never arrives, which would otherwise pin a handler and its
    /// listener for the life of the process.
    pub fn with_bind_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.bind_timeout = timeout;
        self
    }

    /// Sets the maximum number of concurrent connection handlers in
    /// [`serve`](Self::serve). Excess connections wait in the accept backlog
    /// until a slot frees. Must be at least 1.
    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max.max(1);
        self
    }

    /// The address the server is listening on.
    ///
    /// # Errors
    /// Returns an error if the socket address cannot be retrieved.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    /// Accepts and serves a single client on the current thread.
    ///
    /// # Errors
    /// Returns an error if accepting fails or the client's session ends in
    /// an error.
    pub fn accept(&self) -> Result<()> {
        let (stream, _) = self.listener.accept()?;
        handle_client(
            stream,
            &self.authenticators,
            self.handshake_timeout,
            self.bind_timeout,
        )
    }

    /// Serves clients until the listener fails, one thread per connection, up
    /// to [`max_connections`](Self::with_max_connections) concurrently.
    ///
    /// # Errors
    /// Returns an error if accepting a connection fails.
    pub fn serve(&self) -> Result<()> {
        let slots = Semaphore::new(self.max_connections);
        loop {
            let (stream, _) = self.listener.accept()?;
            tracing::debug!(peer = ?stream.peer_addr().ok(), "accepted connection");
            // Throttle here: block the accept loop until a handler slot frees,
            // applying backpressure instead of spawning without bound.
            let permit = slots.acquire();
            let authenticators = Arc::clone(&self.authenticators);
            let handshake_timeout = self.handshake_timeout;
            let bind_timeout = self.bind_timeout;
            thread::spawn(move || {
                let _permit = permit; // released when the handler thread ends
                let _ = handle_client(stream, &authenticators, handshake_timeout, bind_timeout);
            });
        }
    }
}
