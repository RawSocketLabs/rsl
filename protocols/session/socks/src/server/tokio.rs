//! Configured Tokio exchange, authorization, and connection stages.
use crate::io::tokio::{bounded, bounded_until, check_deadline};
use crate::v5::{
    Endpoint,
    server::tokio::{Request, exchange},
};
use crate::{Connection, Server, Stream, error::Error, server::ServerConfig};
use crate::{
    Destination, Version,
    server::policy::{Operation, RequestInfo},
};
use std::{net::SocketAddr, time::Duration};
use tokio::net::TcpStream;
async fn resolve(destination: &Destination) -> Result<Vec<SocketAddr>, Error> {
    Ok(match destination {
        Destination::Ip { address, port } => vec![SocketAddr::from((*address, *port))],
        Destination::Domain { name, port } => {
            tokio::net::lookup_host((crate::io::domain_name(name)?, *port))
                .await?
                .collect()
        }
    })
}

async fn dial(addresses: Vec<SocketAddr>, until: tokio::time::Instant) -> Result<TcpStream, Error> {
    let mut error = Error::InvalidEndpoint;
    for address in addresses {
        check_deadline(until)?;
        match TcpStream::connect(address).await {
            Ok(stream) => return Ok(stream),
            Err(cause) => error = cause.into(),
        }
    }
    Err(error)
}

impl Server {
    /// Authenticate and receive a Tokio TCP request under the handshake deadline.
    /// No destination is authorized or connected and no success reply is sent yet.
    ///
    /// ```no_run
    /// # use socks::{Server, server::policy::ServerAuth, server::ServerConfig};
    /// # async fn example(stream: tokio::net::TcpStream) -> Result<(), socks::error::Error> {
    /// let server = Server::builder()
    ///     .protocols([socks::Version::V5])
    ///     .policy(socks::server::policy::Policy::new(ServerAuth::no_authentication(),
    ///         |context| context.target.ip().is_loopback()))
    ///     .build()?;
    /// let connection = server.exchange_async(stream).await?
    ///     .authorize().await?.connect().await?;
    /// let (client, target) = connection.into_parts();
    /// # drop((client, target));
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    /// Returns terminal TCP, codec, authentication, or request errors.
    /// Cancellation is terminal and drops the owned transport.
    pub async fn exchange_async(&self, stream: TcpStream) -> Result<Exchange<'_>, Error> {
        let peer = stream.peer_addr()?;
        let request = match self.config.protocol() {
            Version::V5 => {
                bounded(
                    self.config.limits.handshake,
                    exchange(stream, self.config.policy.authentication()),
                )
                .await?
            }
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
/// ```compile_fail
/// use socks::{Server, error::Error};
/// async fn bypass(server: &Server, stream: tokio::net::TcpStream) -> Result<(), Error> {
///     let _connection = server.exchange_async(stream).await?.connect().await?;
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
    /// One budget spans DNS, policy evaluation, caller delay, and all dial attempts.
    /// Authorization is a snapshot; policy is not rerun during `connect`.
    /// Cancellation cannot stop a system resolver call already running in Tokio's
    /// blocking pool, but it drops this session and prevents subsequent dialing.
    ///
    /// # Errors
    /// Attempts a fresh bounded failure reply on resolution, timeout, or policy errors.
    /// Reply errors take precedence. Errors and cancellation close the owned transport.
    pub async fn authorize(self) -> Result<Authorized<'a>, Error> {
        let result = async {
            let until = tokio::time::Instant::now()
                .checked_add(self.config.limits.connect)
                .ok_or(Error::InvalidLimits)?;
            let addresses = bounded_until(until, async {
                let addresses = resolve(self.destination()).await?;
                let addresses = self
                    .config
                    .policy
                    .authorize_addresses(self.peer, &self.info, addresses);
                check_deadline(until)?;
                addresses
            })
            .await?;
            Ok((addresses, until))
        }
        .await;
        match result {
            Ok((addresses, until)) => Ok(Authorized {
                exchange: self,
                addresses,
                until,
            }),
            Err(error) => self.fail(error).await,
        }
    }

    async fn fail<T>(self, error: Error) -> Result<T, Error> {
        bounded(
            self.config.limits.handshake.min(Duration::from_secs(10)),
            self.request.send_failure(error.reply_code()),
        )
        .await?;
        Err(error)
    }

    async fn succeed(self, bound: Endpoint) -> Result<Stream<TcpStream>, Error> {
        bounded(
            self.config.limits.handshake.min(Duration::from_secs(10)),
            self.request.send_success(bound),
        )
        .await
    }
}

/// An authorized request with immutable numeric targets and an absolute dial deadline.
/// Only this stage exposes the configured server's outbound `connect` operation.
#[must_use = "connect the authorized request or drop it to close the client"]
pub struct Authorized<'a> {
    exchange: Exchange<'a>,
    addresses: Vec<SocketAddr>,
    until: tokio::time::Instant,
}

impl Authorized<'_> {
    /// Dial a retained permitted address and acknowledge its actual local endpoint.
    /// Returns both connected sides without starting a relay or resolving again.
    ///
    /// # Errors
    /// Attempts a bounded failure reply if dialing fails or its budget expired.
    /// Reply errors take precedence. Errors and cancellation close owned transports.
    pub async fn connect(self) -> Result<Connection<TcpStream>, Error> {
        let result = async {
            let target = bounded_until(self.until, dial(self.addresses, self.until)).await?;
            let bound = target.local_addr()?.into();
            Ok((target, bound))
        }
        .await;
        match result {
            Ok((target, bound)) => Ok(Connection {
                client: self.exchange.succeed(bound).await?,
                target,
            }),
            Err(error) => self.exchange.fail(error).await,
        }
    }
}
