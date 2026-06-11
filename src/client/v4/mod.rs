//! Driving a SOCKS4 / 4A proxy: the client side of the 1992 memo.
//!
//! [`Client`] is the entry point. Unlike SOCKS5 there is no method negotiation:
//! the client sends a single request (optionally identifying itself with a
//! `userid`) and reads a single reply. The returned
//! [`TcpStream`](std::net::TcpStream) is transparent to the destination.
//!
//! | Command | Method | Returns |
//! |---|---|---|
//! | CONNECT (IPv4) | [`Client::connect`] | a transparent [`TcpStream`](std::net::TcpStream) |
//! | CONNECT (domain, 4A) | [`Client::connect_domain`] | a transparent [`TcpStream`](std::net::TcpStream) |
//! | BIND | [`Client::bind`] | a [`BindListener`] that yields the inbound peer |
//!
//! SOCKS4 addresses are **IPv4 only**. SOCKS4A (the `v4a` feature) adds
//! [`connect_domain`](Client::connect_domain), which asks the *proxy* to
//! resolve a host name so DNS need not leak from the client.
//!
//! For deliberately malformed traffic, drop to the [`raw`] module.
//!
//! # Example
//!
//! ```no_run
//! use std::net::Ipv4Addr;
//! use socks::client::v4::Client;
//!
//! # fn main() -> Result<(), socks::error::SocksError> {
//! let stream = Client::new("127.0.0.1:1080".parse()?)
//!     .userid("alice")
//!     .connect((Ipv4Addr::new(93, 184, 216, 34), 80))?;
//! # let _ = stream;
//! # Ok(())
//! # }
//! ```

pub mod raw;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream};

use binrw::{io::NoSeek, BinWrite, NullString};

use crate::error::{Result, SocksError};
use crate::v4::{Command, Reply, RequestBuilder};

/// A SOCKS4 / 4A destination: an IPv4 socket address. (Host names are not a
/// `Target`; they go through [`Client::connect_domain`] under the `v4a`
/// feature, since they are a distinct wire encoding.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// The destination IPv4 address.
    pub ip: Ipv4Addr,
    /// The destination port.
    pub port: u16,
}

impl From<(Ipv4Addr, u16)> for Target {
    fn from((ip, port): (Ipv4Addr, u16)) -> Self {
        Target { ip, port }
    }
}

impl From<SocketAddrV4> for Target {
    fn from(addr: SocketAddrV4) -> Self {
        Target {
            ip: *addr.ip(),
            port: addr.port(),
        }
    }
}

/// A SOCKS4 / 4A client that issues a single command against a proxy.
///
/// Construct one with [`new`](Self::new), optionally attach a
/// [`userid`](Self::userid), then call [`connect`](Self::connect),
/// [`connect_domain`](Self::connect_domain) (4A), or [`bind`](Self::bind). Each
/// command opens a fresh connection to the proxy, so a single `Client` is a
/// reusable description of *how* to reach the proxy, not a live connection.
//~ models socks4#front
#[derive(Clone, Debug)]
pub struct Client {
    /// The proxy's address. Public so it can be inspected or reused.
    pub proxy: SocketAddr,
    /// The identity sent in the request's USERID field. Often empty.
    pub userid: NullString,
}

impl Client {
    /// Creates a client with an empty userid.
    pub fn new(proxy: SocketAddr) -> Self {
        Self {
            proxy,
            userid: NullString::default(),
        }
    }

    /// Sets the userid sent in the request (the SOCKS4 identity field).
    pub fn userid(mut self, userid: impl Into<NullString>) -> Self {
        self.userid = userid.into();
        self
    }

    /// Connects to the proxy, sends one request, and returns the stream
    /// together with the granted reply. Shared by every command.
    fn exchange(
        &self,
        command: Command,
        ip: Ipv4Addr,
        port: u16,
        #[cfg(feature = "v4a")] domain: Option<NullString>,
    ) -> Result<(TcpStream, Reply)> {
        tracing::debug!(proxy = %self.proxy, ?command, "connecting to SOCKS4 proxy");
        //~ implements socks4#front/should.8d142d part="client emits VN=4"
        //~ implements socks4a#front/should.8d142d part="client emits VN=4"
        let stream = TcpStream::connect(self.proxy)?;
        // Disable Nagle: the request/reply handshake is small and strictly
        // alternating, and this same stream becomes the relay. Best-effort.
        let _ = stream.set_nodelay(true);

        let mut builder = RequestBuilder::default();
        builder
            .command(command)
            .dest_ip(ip)
            .dest_port(port)
            .userid(self.userid.clone());
        #[cfg(feature = "v4a")]
        builder.domain(domain);
        let request = builder
            .build()
            .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;
        request.write(&mut NoSeek::new(&stream))?;

        let reply = Reply::read_from(&mut &stream)?;
        Ok((stream, reply))
    }

    /// Establishes a CONNECT tunnel to an IPv4 `target`, returning a stream
    /// transparent to the remote host.
    ///
    /// # Errors
    /// Returns [`SocksError::V4ReplyFailure`] when the proxy refuses (any code
    /// other than 90), or an I/O or parse error.
    //~ implements socks4#front/should.01af87 part="client CONNECT"
    pub fn connect(&self, target: impl Into<Target>) -> Result<TcpStream> {
        let target = target.into();
        let (stream, reply) = self.exchange(
            Command::Connect,
            target.ip,
            target.port,
            #[cfg(feature = "v4a")]
            None,
        )?;
        check_granted(reply)?;
        Ok(stream)
    }

    /// Establishes a CONNECT tunnel by **host name**, letting the proxy resolve
    /// it (SOCKS4A). The client sends the `0.0.0.x` marker address and the host
    /// name; the proxy does the DNS lookup, so it need not leak from the client.
    ///
    /// # Errors
    /// Returns [`SocksError::Validation`] if the host name is empty or longer
    /// than 255 bytes, [`SocksError::V4ReplyFailure`] on refusal, or an I/O or
    /// parse error.
    //~ implements socks4a#front/should.f6d704 part="client sends 0.0.0.x marker"
    //~ implements socks4a#front/must.a1f21e part="client appends NULL-terminated domain after USERID"
    #[cfg(feature = "v4a")]
    pub fn connect_domain(&self, host: &str, port: u16) -> Result<TcpStream> {
        if host.is_empty() || host.len() > 255 {
            return Err(SocksError::Validation(
                "SOCKS4A host name must be 1..=255 bytes".to_string(),
            ));
        }
        // 0.0.0.1 — the inadmissible marker that signals "domain follows".
        let (stream, reply) = self.exchange(
            Command::Connect,
            Ipv4Addr::new(0, 0, 0, 1),
            port,
            Some(NullString::from(host)),
        )?;
        check_granted(reply)?;
        Ok(stream)
    }

    /// Asks the proxy to listen for an inbound connection on the client's
    /// behalf — the BIND command. `expected_peer` names the IPv4 address the
    /// proxy should expect the inbound connection from (per the memo, BIND is
    /// used only after a primary CONNECT to the same destination).
    ///
    /// Returns a [`BindListener`] carrying the address the proxy is now
    /// listening on; call [`BindListener::accept`] to block until the peer
    /// connects.
    ///
    /// # Errors
    /// Returns [`SocksError::V4ReplyFailure`] when the proxy refuses, or an I/O
    /// or parse error.
    //~ implements socks4#front/must.6e79ac part="client BIND"
    pub fn bind(&self, expected_peer: impl Into<Target>) -> Result<BindListener> {
        let peer = expected_peer.into();
        let (stream, reply) = self.exchange(
            Command::Bind,
            peer.ip,
            peer.port,
            #[cfg(feature = "v4a")]
            None,
        )?;
        let reply = check_granted(reply)?;
        let proxy_ip = match self.proxy {
            SocketAddr::V4(addr) => *addr.ip(),
            SocketAddr::V6(_) => Ipv4Addr::UNSPECIFIED,
        };
        // `bound_socket` substitutes the proxy IP for INADDR_ANY (socks4
        // should.c1f159), annotated at its definition in `v4::Reply`.
        let bound = reply.bound_socket(proxy_ip);
        Ok(BindListener { stream, bound })
    }
}

/// A pending SOCKS4 BIND command.
///
/// The proxy is listening on [`BindListener::bound`]; the expected peer should
/// connect there. Call [`BindListener::accept`] to wait for it (the proxy sends
/// a second reply once the peer arrives).
pub struct BindListener {
    stream: TcpStream,
    /// The address the proxy is listening on (with `INADDR_ANY` already
    /// resolved to the proxy's address).
    pub bound: SocketAddrV4,
}

impl BindListener {
    /// Blocks until the expected peer connects to the proxy, returning the
    /// now-transparent stream and the peer's address.
    ///
    /// # Errors
    /// Returns [`SocksError::V4ReplyFailure`] when the proxy reports a failure,
    /// or an I/O or parse error.
    pub fn accept(self) -> Result<(TcpStream, SocketAddrV4)> {
        let reply = Reply::read_from(&mut &self.stream)?;
        let reply = check_granted(reply)?;
        let peer = reply.bound_socket(*self.bound.ip());
        Ok((self.stream, peer))
    }
}

/// Returns the reply if it granted the request, else [`SocksError::V4ReplyFailure`].
fn check_granted(reply: Reply) -> Result<Reply> {
    if reply.code.is_granted() {
        Ok(reply)
    } else {
        Err(SocksError::V4ReplyFailure(reply.code))
    }
}
