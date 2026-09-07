//! Tokio CONNECT sessions and a bounded listening proxy (`tokio` feature).
//!
//! APIs mirror [`crate::session`]'s blocking policies and use `AsyncRead + AsyncWrite`.
//! Generic handshakes have caller-managed deadlines. Cancellation is terminal:
//! discard the transport after cancelling a handshake, reply, or relay, including
//! when it was borrowed. No partially consumed handshake can be resumed.
//!
//! Complete listener (`tokio` feature), stopped by an application-owned channel:
//! ```no_run
//! use socks::{asynchronous, session::{ServerAuth, ServerConfig}};
//! use std::net::SocketAddr;
//! use tokio::{net::TcpListener, sync::oneshot};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let allowed: SocketAddr = "127.0.0.1:8080".parse()?;
//! let config = ServerConfig::new(ServerAuth::no_authentication(),
//!     move |_peer, _requested, resolved| resolved == allowed);
//! let listener = TcpListener::bind("127.0.0.1:1080").await?;
//! let (stop, stopped) = oneshot::channel::<()>();
//! // Give stop to your application controller; sending () initiates shutdown.
//! asynchronous::serve(listener, &config, async { let _ = stopped.await; },
//!     |_error| { /* report without secrets */ }).await?;
//! # drop(stop);
//! # Ok(()) }
//! ```
//!
//! [`connect_tcp`] returns a Tokio stream ready for the tunneled protocol.
//! [`connect`] and [`accept`] instead compose over any `AsyncRead + AsyncWrite + Unpin`,
//! including protected transports. Embedded servers must authorize and dial before
//! [`Incoming::accept`]; they own subsequent relay, deadlines, and shutdown.

use crate::error::Error;
use crate::session::{self, ClientAuth, Frame, ServerAuth, ServerConfig};
use crate::v5::{
    AuthMethod, Command, Endpoint, MethodRequest, MethodSelection, Reply, ReplyCode, Request,
    UsernamePasswordRequest, UsernamePasswordResponse, UsernamePasswordStatus, VERSION,
};
use bnb::{BitDecode, BitEncode};
use std::future::Future;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;

async fn read<S: AsyncRead + Unpin, T: BitDecode + BitEncode>(
    stream: &mut S,
    frame: Frame,
) -> Result<T, Error> {
    let mut bytes = Vec::new();
    loop {
        let needed = frame.needed(&bytes)?;
        let previous = bytes.len();
        if previous == needed {
            return Ok(bnb::bitstream::decode_exact(&bytes, T::LAYOUT)?);
        }
        bytes.resize(needed, 0);
        stream.read_exact(&mut bytes[previous..]).await?;
    }
}

async fn write<S: AsyncWrite + Unpin, T: BitEncode>(
    stream: &mut S,
    message: &T,
) -> Result<(), Error> {
    stream
        .write_all(&bnb::bitstream::encode_to_vec(message, T::LAYOUT)?)
        .await?;
    stream.flush().await?;
    Ok(())
}

async fn bounded<T>(
    duration: Duration,
    work: impl Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    if duration.is_zero() || Instant::now().checked_add(duration).is_none() {
        return Err(Error::InvalidLimits);
    }
    tokio::time::timeout(duration, work)
        .await
        .map_err(|_| Error::Io(session::timed_out()))?
}

/// Negotiate CONNECT and return the transport and proxy-bound endpoint. Domains
/// go to the proxy unchanged, without local DNS. No tunnel bytes are read ahead.
///
/// # Errors
/// Returns a terminal transport, codec, authentication, or peer-reply error.
/// Invalid local arguments fail before writing any handshake bytes.
pub async fn connect<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    destination: Endpoint,
    auth: ClientAuth<'_>,
) -> Result<(S, Endpoint), Error> {
    session::endpoint(&destination)?;
    let credentials = auth.credentials()?;
    connect_prepared(stream, destination, auth.method(), credentials).await
}

async fn connect_prepared<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    destination: Endpoint,
    method: AuthMethod,
    credentials: Option<UsernamePasswordRequest>,
) -> Result<(S, Endpoint), Error> {
    write(
        &mut stream,
        &MethodRequest {
            version: VERSION,
            methods: vec![method],
        },
    )
    .await?;
    let selected: MethodSelection = read(&mut stream, Frame::Selection).await?;
    session::selection(selected, method)?;
    if let Some(credentials) = credentials {
        write(&mut stream, &credentials).await?;
        session::authentication(&read(&mut stream, Frame::AuthReply).await?)?;
    }
    write(
        &mut stream,
        &Request {
            version: VERSION,
            reserved: 0,
            command: Command::Connect,
            destination,
        },
    )
    .await?;
    let response: Reply = read(&mut stream, Frame::Command).await?;
    session::reply(&response)?;
    Ok((stream, response.bound))
}

/// Dial a numeric proxy address and negotiate within one absolute timeout.
///
/// # Errors
/// Returns a terminal connection/session error, including an invalid timeout.
pub async fn connect_tcp(
    proxy: SocketAddr,
    destination: Endpoint,
    auth: ClientAuth<'_>,
    timeout: Duration,
) -> Result<(TcpStream, Endpoint), Error> {
    session::endpoint(&destination)?;
    let credentials = auth.credentials()?;
    bounded(timeout, async {
        let stream = TcpStream::connect(proxy).await?;
        connect_prepared(stream, destination, auth.method(), credentials).await
    })
    .await
}

/// An authenticated CONNECT request awaiting authorization and a target connection.
pub struct Incoming<S> {
    stream: S,
    destination: Endpoint,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Incoming<S> {
    /// The original destination, including unresolved domain bytes.
    pub fn destination(&self) -> &Endpoint {
        &self.destination
    }

    /// Acknowledge an established target connection, then return the untouched tunnel.
    /// `bound` must be the target connection's local endpoint.
    ///
    /// # Errors
    /// Returns a terminal codec or transport error. Cancellation is also terminal.
    pub async fn accept(mut self, bound: Endpoint) -> Result<S, Error> {
        session::endpoint(&bound)?;
        write(
            &mut self.stream,
            &session::response(ReplyCode::Succeeded, bound),
        )
        .await?;
        Ok(self.stream)
    }

    /// Send a failing reply and drop the transport. Success codes are rejected.
    ///
    /// # Errors
    /// Returns an I/O/codec error or rejects an attempted success reply.
    pub async fn reject(mut self, code: ReplyCode) -> Result<(), Error> {
        if u8::from(code) == 0 {
            return Err(Error::Reply(code));
        }
        write(&mut self.stream, &session::failure(code)).await
    }
}

/// Authenticate and read a CONNECT request without dialing or authorizing it.
/// Callers manage deadlines; credential callbacks must not block the runtime.
///
/// # Errors
/// Returns terminal handshake errors; unsupported commands/address types receive
/// a SOCKS failure reply. Cancelling the future requires discarding the transport.
pub async fn accept<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    auth: &ServerAuth,
) -> Result<Incoming<S>, Error> {
    let offer: MethodRequest = read(&mut stream, Frame::Offer).await?;
    let selected = auth.select(&offer);
    write(
        &mut stream,
        &MethodSelection {
            version: VERSION,
            method: selected
                .as_ref()
                .copied()
                .unwrap_or(AuthMethod::NoAcceptable),
        },
    )
    .await?;
    selected?;
    if auth.method() == AuthMethod::UsernamePassword {
        let credentials: UsernamePasswordRequest = read(&mut stream, Frame::Credentials).await?;
        let accepted = auth.authenticate(&credentials);
        write(
            &mut stream,
            &UsernamePasswordResponse {
                version: 1,
                status: UsernamePasswordStatus::from(u8::from(!accepted)),
            },
        )
        .await?;
        if !accepted {
            return Err(Error::AuthenticationRejected);
        }
    }
    let result = read::<_, Request>(&mut stream, Frame::Command)
        .await
        .and_then(|request| {
            session::request(&request)?;
            Ok(request)
        });
    match result {
        Ok(request) => Ok(Incoming {
            stream,
            destination: request.destination,
        }),
        Err(error) => {
            write(&mut stream, &session::failure(error.reply_code())).await?;
            Err(error)
        }
    }
}

async fn dial(
    destination: &Endpoint,
    peer: SocketAddr,
    config: &ServerConfig,
) -> Result<TcpStream, Error> {
    let addresses: Vec<_> = match destination {
        Endpoint::Ipv4 { address, port } => vec![SocketAddr::from((*address, *port))],
        Endpoint::Ipv6 { address, port } => vec![SocketAddr::from((*address, *port))],
        Endpoint::Domain { name, port } => {
            tokio::net::lookup_host((session::domain_name(name)?, *port))
                .await?
                .collect()
        }
    };
    let mut error = if addresses.is_empty() {
        Error::InvalidEndpoint
    } else {
        Error::PermissionDenied
    };
    for address in addresses {
        if config.permits(peer, destination, address) {
            match TcpStream::connect(address).await {
                Ok(stream) => return Ok(stream),
                Err(cause) => error = cause.into(),
            }
        }
    }
    Err(error)
}

/// Run a complete TCP proxy session with shared policy, phase deadlines, and
/// bidirectional half-close-aware relay. Async DNS is awaited within the connect
/// budget; cancellation cannot stop a system resolver call already running in
/// Tokio's blocking pool. Authentication/authorization callbacks must not block.
///
/// # Errors
/// Returns a terminal handshake, target, timeout, or relay error.
pub async fn serve_connection(stream: TcpStream, config: &ServerConfig) -> Result<(), Error> {
    config.limits.validate()?;
    let peer = stream.peer_addr()?;
    let incoming = bounded(config.limits.handshake, accept(stream, &config.auth)).await?;
    let target = bounded(
        config.limits.connect,
        dial(incoming.destination(), peer, config),
    )
    .await;
    let reply_timeout = config.limits.handshake.min(Duration::from_secs(10));
    match target {
        Ok(mut target) => {
            let bound = target.local_addr()?.into();
            let mut client = bounded(reply_timeout, incoming.accept(bound)).await?;
            bounded(config.limits.relay, async {
                tokio::io::copy_bidirectional(&mut client, &mut target).await?;
                Ok(())
            })
            .await
        }
        Err(error) => {
            bounded(reply_timeout, incoming.reject(error.reply_code())).await?;
            Err(error)
        }
    }
}

/// Serve a TCP listener with bounded task concurrency until `shutdown` completes.
/// Stop acceptance, abort and join active session tasks on shutdown or listener
/// failure. Individual session failures go to `on_error`; they do not stop serving.
/// Saturation leaves connections in the OS backlog. Callbacks must not panic/block.
///
/// # Errors
/// Returns invalid limits, a listener error, or a worker panic.
pub async fn serve(
    listener: TcpListener,
    config: &ServerConfig,
    shutdown: impl Future<Output = ()>,
    on_error: impl Fn(Error),
) -> Result<(), Error> {
    config.limits.validate()?;
    let mut workers = JoinSet::new();
    tokio::pin!(shutdown);
    let result = loop {
        tokio::select! {
            biased;
            () = &mut shutdown => break Ok(()),
            result = workers.join_next(), if !workers.is_empty() => {
                match result {
                    Some(Ok(Err(error))) => on_error(error),
                    Some(Err(_)) => break Err(Error::WorkerPanicked),
                    _ => {}
                }
            }
            accepted = listener.accept(), if workers.len() < config.limits.connections => {
                match accepted {
                    Ok((stream, _)) => {
                        let config = config.clone();
                        workers.spawn(async move { serve_connection(stream, &config).await });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => break Err(error.into()),
                }
            }
        }
    };
    drop(listener);
    workers.abort_all();
    while workers.join_next().await.is_some() {}
    result
}
