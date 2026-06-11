//! Serving SOCKS4 / 4A clients: the proxy side of the 1992 memo.
//!
//! [`Server`] binds a listener and, for each client, reads one request, decides
//! whether to grant it, performs the CONNECT or BIND, and relays bytes until
//! either side closes. There is no method negotiation or authentication
//! subnegotiation — the only identity is the request's `USERID` field, which an
//! [authorizer](Server::with_authorizer) callback may inspect (alongside the
//! destination) to accept or reject the request.
//!
//! Two ways to run it, mirroring the SOCKS5 [`Server`](crate::server::Server):
//!
//! - [`Server::accept`] serves exactly one client on the current thread.
//! - [`Server::serve`] loops forever, one thread per connection, bounded by
//!   [`with_max_connections`](Server::with_max_connections).
//!
//! Under the `v4a` feature the server also honors the `0.0.0.x` marker: when a
//! request carries it, the server reads the trailing host name and resolves it
//! itself before connecting.
//!
//! # Example
//!
//! ```no_run
//! use socks::server::v4::Server;
//!
//! # fn main() -> Result<(), socks::error::SocksError> {
//! // Accept only clients identifying as "alice".
//! Server::bind("127.0.0.1:1080")?
//!     .with_authorizer(|req| req.userid.to_string() == "alice")
//!     .serve()?;
//! # Ok(())
//! # }
//! ```

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use binrw::{io::NoSeek, BinWrite};

use crate::error::{Result, SocksError};
use crate::server::pool::{accept_with_timeout, Semaphore};
use crate::server::relay::relay;
use crate::v4::{Command, ReplyBuilder, ReplyCode, Request};

/// Default deadline for a client to send its request before the server drops
/// it. The SOCKS4 handshake is a single request, so this guards a client that
/// connects and then stalls.
pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Default deadline for a BIND request to receive its inbound peer connection.
pub const DEFAULT_BIND_TIMEOUT: Duration = Duration::from_secs(120);

/// Default cap on concurrent connection handlers in [`Server::serve`].
pub const DEFAULT_MAX_CONNECTIONS: usize = 1024;

/// A predicate deciding whether to grant a request, given the full parsed
/// [`Request`] (so it can inspect the userid and destination).
type Authorizer = dyn Fn(&Request) -> bool + Send + Sync;

/// A SOCKS4 / 4A proxy server.
///
/// Configure it with the builder-style setters (each consumes and returns
/// `self`), then drive it with [`serve`](Self::serve) or
/// [`accept`](Self::accept). By default every request is granted; restrict with
/// [`with_authorizer`](Self::with_authorizer).
//~ models socks4#front
pub struct Server {
    listener: TcpListener,
    authorizer: Arc<Authorizer>,
    handshake_timeout: Option<Duration>,
    bind_timeout: Option<Duration>,
    max_connections: usize,
}

impl Server {
    /// Binds a server that grants every request.
    ///
    /// # Errors
    /// Returns an error if the listener cannot be bound.
    pub fn bind(addr: impl ToSocketAddrs) -> Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(addr)?,
            authorizer: Arc::new(|_| true),
            handshake_timeout: Some(DEFAULT_HANDSHAKE_TIMEOUT),
            bind_timeout: Some(DEFAULT_BIND_TIMEOUT),
            max_connections: DEFAULT_MAX_CONNECTIONS,
        })
    }

    /// Sets the predicate that decides whether to grant a request. It receives
    /// the parsed [`Request`] — including the userid and destination — and
    /// returns `true` to grant, `false` to reject with code 91. This is the
    /// access-control hook the memo describes (checks on source/destination,
    /// userid, and identd).
    //~ implements socks4#front/should.3869f5 part="server access-control check"
    pub fn with_authorizer<F>(mut self, authorizer: F) -> Self
    where
        F: Fn(&Request) -> bool + Send + Sync + 'static,
    {
        self.authorizer = Arc::new(authorizer);
        self
    }

    /// Sets the deadline for the client to send its request. `None` disables it.
    pub fn with_handshake_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    /// Sets the deadline for a BIND request to receive its inbound peer
    /// connection. `None` waits indefinitely.
    pub fn with_bind_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.bind_timeout = timeout;
        self
    }

    /// Sets the maximum number of concurrent connection handlers. Must be at
    /// least 1.
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
    /// Returns an error if accepting fails or the client's session ends in an
    /// error.
    pub fn accept(&self) -> Result<()> {
        let (stream, _) = self.listener.accept()?;
        handle_client(
            stream,
            &*self.authorizer,
            self.handshake_timeout,
            self.bind_timeout,
        )
    }

    /// Serves clients until the listener fails, one thread per connection, up to
    /// [`max_connections`](Self::with_max_connections) concurrently.
    ///
    /// # Errors
    /// Returns an error if accepting a connection fails.
    pub fn serve(&self) -> Result<()> {
        let slots = Semaphore::new(self.max_connections);
        loop {
            let (stream, _) = self.listener.accept()?;
            tracing::debug!(peer = ?stream.peer_addr().ok(), "accepted SOCKS4 connection");
            let permit = slots.acquire();
            let authorizer = Arc::clone(&self.authorizer);
            let handshake_timeout = self.handshake_timeout;
            let bind_timeout = self.bind_timeout;
            thread::spawn(move || {
                let _permit = permit; // released when the handler thread ends
                let _ = handle_client(stream, &*authorizer, handshake_timeout, bind_timeout);
            });
        }
    }
}

/// Serves a single client connection: request, authorization, command dispatch,
/// and relaying.
#[tracing::instrument(
    name = "socks4.connection",
    skip_all,
    fields(peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default())
)]
fn handle_client(
    stream: TcpStream,
    authorizer: &Authorizer,
    handshake_timeout: Option<Duration>,
    bind_timeout: Option<Duration>,
) -> Result<()> {
    tracing::debug!("handling SOCKS4 connection");
    if let Some(timeout) = handshake_timeout {
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
    }

    //~ implements socks4a#front/must.febee7 part="server reads and checks the request"
    let request = Request::read_from(&mut &stream)?;
    if request.version != 4 {
        tracing::warn!(version = request.version, "rejecting non-v4 request");
        let _ = send_reply(&stream, ReplyCode::Rejected, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)));
        return Err(SocksError::UnsupportedVersion(request.version));
    }

    if !authorizer(&request) {
        tracing::warn!(userid = %request.userid, "request rejected by authorizer");
        send_reply(&stream, ReplyCode::Rejected, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
        return Err(SocksError::Validation("SOCKS4 request not authorized".to_string()));
    }

    // The handshake is complete; an established relay may idle indefinitely.
    if handshake_timeout.is_some() {
        stream.set_read_timeout(None)?;
        stream.set_write_timeout(None)?;
    }

    tracing::info!(command = ?request.command, "dispatching SOCKS4 request");
    match request.command {
        Command::Connect => handle_connect(stream, &request),
        Command::Bind => handle_bind(stream, &request, bind_timeout),
        Command::Custom(other) => {
            tracing::warn!(command = other, "unsupported SOCKS4 command");
            send_reply(&stream, ReplyCode::Rejected, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
            Err(SocksError::NotSupported(format!("SOCKS4 command {other}")))
        }
    }
}

/// Resolves the destination a request names into one or more socket addresses.
/// For a plain IPv4 target that is the address itself; under `v4a`, when the
/// `0.0.0.x` marker is present, it is the host name the request carries.
//~ implements socks4a#front/should.b0beae part="server resolves the domain"
fn resolve_target(request: &Request) -> Result<Vec<SocketAddr>> {
    #[cfg(feature = "v4a")]
    if crate::v4::is_unresolved_marker(&request.dest_ip) {
        let host = match &request.domain {
            Some(domain) => domain.to_string(),
            None => {
                return Err(SocksError::MessageParse(
                    "SOCKS4A marker address without a domain name".to_string(),
                ))
            }
        };
        let resolved: Vec<SocketAddr> = (host.as_str(), request.dest_port)
            .to_socket_addrs()?
            .collect();
        return Ok(resolved);
    }
    Ok(vec![SocketAddr::from((request.dest_ip, request.dest_port))])
}

fn handle_connect(stream: TcpStream, request: &Request) -> Result<()> {
    let targets = match resolve_target(request) {
        Ok(targets) if !targets.is_empty() => targets,
        _ => {
            send_reply(&stream, ReplyCode::Rejected, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
            return Err(SocksError::Validation(
                "SOCKS4 destination did not resolve".to_string(),
            ));
        }
    };

    let conn = match connect_any(&targets) {
        Ok(conn) => conn,
        Err(err) => {
            send_reply(&stream, ReplyCode::Rejected, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
            return Err(err.into());
        }
    };

    send_reply(&stream, ReplyCode::Granted, conn.peer_addr().unwrap_or_else(|_| targets[0]))?;
    relay(stream, conn)
}

/// Tries each resolved address in turn, returning the first that connects.
fn connect_any(targets: &[SocketAddr]) -> io::Result<TcpStream> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "no addresses to connect");
    for target in targets {
        match TcpStream::connect(target) {
            Ok(conn) => return Ok(conn),
            Err(err) => last = err,
        }
    }
    Err(last)
}

//~ implements socks4#front/should.8dd18d part="server bind/listen/accept procedure"
fn handle_bind(stream: TcpStream, request: &Request, bind_timeout: Option<Duration>) -> Result<()> {
    let listener = match TcpListener::bind((stream.local_addr()?.ip(), 0)) {
        Ok(listener) => listener,
        Err(err) => {
            send_reply(&stream, ReplyCode::Rejected, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
            return Err(err.into());
        }
    };

    // First reply: the address the client should tell the application server to
    // dial back to.
    send_reply(&stream, ReplyCode::Granted, listener.local_addr()?)?;

    let (conn, peer) = match accept_with_timeout(&listener, bind_timeout) {
        Ok(accepted) => accepted,
        Err(err) if err.kind() == io::ErrorKind::TimedOut => {
            send_reply(&stream, ReplyCode::Rejected, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
            return Err(err.into());
        }
        Err(err) => {
            send_reply(&stream, ReplyCode::Rejected, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
            return Err(err.into());
        }
    };

    // Verify the inbound peer matches the address the request named, when that
    // address is concrete (the memo expects BIND's peer to be the CONNECT
    // destination).
    if !request.dest_ip.is_unspecified() && IpAddr::V4(request.dest_ip) != peer.ip() {
        send_reply(&stream, ReplyCode::Rejected, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
        return Err(SocksError::Validation(format!(
            "unexpected SOCKS4 bind peer: {peer}"
        )));
    }

    // Second reply: the peer that connected.
    send_reply(&stream, ReplyCode::Granted, peer)?;
    relay(stream, conn)
}

/// Sends an 8-byte reply carrying `code` and the IPv4 portion of `addr` (a
/// non-IPv4 address degrades to `INADDR_ANY`, which the SOCKS4 reply format
/// cannot otherwise express).
fn send_reply(stream: &TcpStream, code: ReplyCode, addr: SocketAddr) -> Result<()> {
    let (ip, port) = match addr {
        SocketAddr::V4(a) => (*a.ip(), a.port()),
        SocketAddr::V6(a) => (Ipv4Addr::UNSPECIFIED, a.port()),
    };
    //~ implements socks4#front/should.6ba535 part="server emits reply VN=0"
    let reply = ReplyBuilder::default()
        .code(code)
        .dest_ip(ip)
        .dest_port(port)
        .build()
        .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;
    reply.write(&mut NoSeek::new(stream))?;
    Ok(())
}
