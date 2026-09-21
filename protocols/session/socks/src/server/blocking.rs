//! Configured blocking exchange, authorization, and connection stages.
use crate::io::blocking::{Deadline, deadline, remaining};
use crate::v5::{
    Endpoint,
    server::blocking::{Request, exchange},
};
use crate::{Connection, Server, Stream, error::Error, server::ServerConfig};
use crate::{
    Destination, Version,
    server::policy::{Operation, RequestInfo},
};
use std::{net::SocketAddr, time::Duration};
use std::{
    net::{TcpStream, ToSocketAddrs},
    time::Instant,
};
fn resolve(destination: &Destination) -> Result<Vec<SocketAddr>, Error> {
    match destination {
        Destination::Ip { address, port } => Ok(vec![SocketAddr::from((*address, *port))]),
        Destination::Domain { name, port } => Ok((crate::io::domain_name(name)?, *port)
            .to_socket_addrs()?
            .collect()),
    }
}

fn dial(addresses: Vec<SocketAddr>, until: Instant) -> Result<TcpStream, Error> {
    let mut error = Error::InvalidEndpoint;
    for address in addresses {
        match TcpStream::connect_timeout(&address, remaining(until)?) {
            Ok(stream) => return Ok(stream),
            Err(cause) => error = cause.into(),
        }
    }
    Err(error)
}

impl Server {
    /// Authenticate and receive a TCP request under the configured handshake deadline.
    /// No destination is authorized or connected, and no success reply is sent yet.
    /// The supplied socket must be in blocking mode.
    ///
    /// ```no_run
    /// # use socks::{Server, server::policy::ServerAuth, server::ServerConfig};
    /// # use std::net::TcpStream;
    /// # fn example(stream: TcpStream) -> Result<(), socks::error::Error> {
    /// let server = Server::builder()
    ///     .protocols([socks::Version::V5])
    ///     .policy(socks::server::policy::Policy::new(ServerAuth::no_authentication(),
    ///         |context| context.target.ip().is_loopback()))
    ///     .build()?;
    /// let connection = server.exchange(stream)?.authorize()?.connect()?;
    /// let (client, target) = connection.into_parts();
    /// // Hand both sides to application I/O, keeping the client's Stream wrapper.
    /// # drop((client, target));
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    /// Returns terminal TCP, codec, authentication, or request errors. Owned
    /// sockets are dropped on failure. TCP handshake timeouts are cleared on success.
    pub fn exchange(&self, stream: TcpStream) -> Result<Exchange<'_>, Error> {
        let peer = stream.peer_addr()?;
        let request = match self.config.protocol() {
            Version::V5 => exchange(
                Deadline {
                    stream,
                    deadline: deadline(self.config.limits.handshake)?,
                },
                self.config.policy.authentication(),
            )?
            .without_deadline()?,
        };
        let info = RequestInfo {
            version: self.config.protocol(),
            authentication: request.authentication(),
            operation: Operation::Connect,
            destination: request.destination().clone().into(),
        };
        Ok(Exchange {
            info,
            request,
            peer,
            config: &self.config,
        })
    }
}

/// A received request associated with the server's destination policy.
/// Authentication has completed; destination authorization and dialing have not.
///
/// The managed path cannot dial before authorization:
/// ```compile_fail
/// use socks::{Server, error::Error};
/// fn bypass(server: &Server, stream: std::net::TcpStream) -> Result<(), Error> {
///     let _connection = server.exchange(stream)?.connect()?;
///     Ok(())
/// }
/// ```
#[must_use = "authorize the exchange or take the request for manual handling"]
pub struct Exchange<'a> {
    request: Request<TcpStream>,
    info: RequestInfo,
    peer: SocketAddr,
    config: &'a ServerConfig,
}

impl<'a> Exchange<'a> {
    /// The requested destination, before any local resolution.
    #[must_use]
    pub fn destination(&self) -> &Destination {
        &self.info.destination
    }

    /// Protocol and authentication facts supplied to the shared policy.
    #[must_use]
    pub fn request(&self) -> &RequestInfo {
        &self.info
    }

    /// Take manual ownership of authorization, dialing, replies, and deadlines.
    /// No success reply is sent. Prefetched input stays with the request.
    #[must_use]
    pub fn into_request(self) -> Request<TcpStream> {
        self.request
    }

    /// Resolve once and retain only numeric addresses permitted by explicit policy.
    /// System DNS is synchronous, uncancellable, and outside the connection budget.
    /// The budget then spans policy evaluation, caller delay, and all dial attempts.
    /// Authorization is a snapshot; policy is not rerun during `connect`.
    ///
    /// # Errors
    /// Attempts a bounded failure reply on resolution or authorization failure.
    /// A reply I/O error takes precedence over the original error. Failures are terminal.
    pub fn authorize(self) -> Result<Authorized<'a>, Error> {
        let result = (|| {
            let addresses = resolve(self.destination())?;
            let until = deadline(self.config.limits.connect)?;
            let addresses = self
                .config
                .policy
                .authorize_addresses(self.peer, &self.info, addresses);
            remaining(until)?;
            Ok((addresses?, until))
        })();
        match result {
            Ok((addresses, until)) => Ok(Authorized {
                exchange: self,
                addresses,
                until,
            }),
            Err(error) => self.fail(error),
        }
    }

    fn fail<T>(self, error: Error) -> Result<T, Error> {
        self.request
            .bounded(self.config.limits.handshake.min(Duration::from_secs(10)))?
            .send_failure(error.reply_code())?;
        Err(error)
    }

    fn succeed(self, bound: Endpoint) -> Result<Stream<TcpStream>, Error> {
        let stream = self
            .request
            .bounded(self.config.limits.handshake.min(Duration::from_secs(10)))?
            .send_success(bound)?;
        let (stream, buffered) = stream.into_parts();
        stream.stream.set_read_timeout(None)?;
        stream.stream.set_write_timeout(None)?;
        Ok(Stream::from_parts(stream.stream, buffered))
    }
}

/// An authorized request with immutable numeric targets and an absolute dial deadline.
/// Only this stage exposes the configured server's outbound `connect` operation.
///
/// The connection attempt consumes the authorization; it cannot be reused:
/// ```compile_fail
/// use socks::{server::blocking::Authorized, error::Error};
/// fn twice(request: Authorized<'_>) -> Result<(), Error> {
///     let _first = request.connect()?;
///     let _second = request.connect()?;
///     Ok(())
/// }
/// ```
#[must_use = "connect the authorized request or drop it to close the client"]
pub struct Authorized<'a> {
    exchange: Exchange<'a>,
    addresses: Vec<SocketAddr>,
    until: Instant,
}

impl Authorized<'_> {
    /// Dial a retained permitted address and acknowledge its actual local endpoint.
    /// Returns both connected sides without starting a relay or resolving again.
    ///
    /// # Errors
    /// Attempts a bounded failure reply if dialing fails or its budget expired.
    /// A reply error takes precedence; all failures close owned transports.
    pub fn connect(self) -> Result<Connection<TcpStream>, Error> {
        self.connect_with(|_| Ok(()))
    }

    pub(crate) fn connect_with(
        self,
        register_target: impl FnOnce(&TcpStream) -> Result<(), Error>,
    ) -> Result<Connection<TcpStream>, Error> {
        let result = (|| {
            let target = dial(self.addresses, self.until)?;
            let bound = target.local_addr()?.into();
            register_target(&target)?;
            Ok((target, bound))
        })();
        match result {
            Ok((target, bound)) => Ok(Connection {
                client: self.exchange.succeed(bound)?,
                target,
            }),
            Err(error) => self.exchange.fail(error),
        }
    }
}
