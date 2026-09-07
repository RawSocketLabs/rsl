//! Blocking CONNECT sessions and a bounded TCP listening proxy.
//!
//! [`connect`] and [`accept`] consume any `Read + Write` transport. On error the
//! transport is dropped; borrowed transports must be closed by their owner. They
//! do not impose deadlines on generic I/O. [`connect_tcp`] and [`serve_connection`]
//! supply TCP deadlines. No method reads beyond the current handshake message.
//!
//! Client (`blocking` feature):
//! ```no_run
//! use socks::{blocking, session::ClientAuth, v5::Endpoint};
//! use std::{io::Write, time::Duration};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let (mut tunnel, _bound) = blocking::connect_tcp(
//!     "127.0.0.1:1080".parse()?, Endpoint::domain(b"example.com", 443),
//!     ClientAuth::NoAuthentication, Duration::from_secs(10),
//! )?;
//! // Hand tunnel to TLS or another application protocol; no handshake bytes remain.
//! tunnel.write_all(b"application bytes")?;
//! # Ok(()) }
//! ```
//!
//! Complete listener with an explicit target allowlist:
//! ```no_run
//! use socks::{blocking, session::{ServerAuth, ServerConfig}};
//! use std::{net::{TcpListener, SocketAddr}, sync::{Arc, atomic::{AtomicBool, Ordering}}};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let allowed: SocketAddr = "127.0.0.1:8080".parse()?;
//! let config = ServerConfig::new(ServerAuth::no_authentication(),
//!     move |_peer, _requested, resolved| resolved == allowed);
//! let stop = Arc::new(AtomicBool::new(false));
//! // A signal handler or controller sets stop.store(true, Ordering::Release).
//! let listener = TcpListener::bind("127.0.0.1:1080")?;
//! blocking::serve(listener, &config, &stop, |_error| { /* report without secrets */ })?;
//! # Ok(()) }
//! ```
//!
//! To embed only the handshake, call [`accept`], inspect [`Incoming::destination`],
//! apply your policy and connect the target, then call [`Incoming::accept`] with
//! the target socket's local address. That returns the original duplex transport;
//! your application owns relay and shutdown. Never acknowledge success before dial.

use crate::error::Error;
use crate::session::{self, ClientAuth, Frame, ServerAuth, ServerConfig};
use crate::v5::{
    Command, Endpoint, MethodRequest, MethodSelection, Reply, ReplyCode, Request,
    UsernamePasswordRequest, UsernamePasswordResponse, UsernamePasswordStatus, VERSION,
};
use bnb::{BitDecode, BitEncode};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn read<S: Read, T: BitDecode + BitEncode>(stream: &mut S, frame: Frame) -> Result<T, Error> {
    let mut bytes = Vec::new();
    loop {
        let needed = frame.needed(&bytes)?;
        let previous = bytes.len();
        if previous == needed {
            return Ok(bnb::bitstream::decode_exact(&bytes, T::LAYOUT)?);
        }
        bytes.resize(needed, 0);
        stream.read_exact(&mut bytes[previous..])?;
    }
}

fn write<S: Write, T: BitEncode>(stream: &mut S, message: &T) -> Result<(), Error> {
    stream.write_all(&bnb::bitstream::encode_to_vec(message, T::LAYOUT)?)?;
    stream.flush()?;
    Ok(())
}

/// Negotiate one CONNECT request and return the untouched tunnel transport and
/// the proxy's bound endpoint. Domains are sent to the proxy, not resolved locally.
///
/// # Errors
/// Returns a terminal I/O, codec, authentication, or peer-reply error. Invalid
/// local arguments fail before writing any handshake bytes.
pub fn connect<S: Read + Write>(
    stream: S,
    destination: Endpoint,
    auth: ClientAuth<'_>,
) -> Result<(S, Endpoint), Error> {
    session::endpoint(&destination)?;
    let credentials = auth.credentials()?;
    connect_prepared(stream, destination, auth.method(), credentials)
}

fn connect_prepared<S: Read + Write>(
    mut stream: S,
    destination: Endpoint,
    method: crate::v5::AuthMethod,
    credentials: Option<UsernamePasswordRequest>,
) -> Result<(S, Endpoint), Error> {
    write(
        &mut stream,
        &MethodRequest {
            version: VERSION,
            methods: vec![method],
        },
    )?;
    let selected: MethodSelection = read(&mut stream, Frame::Selection)?;
    session::selection(selected, method)?;
    if let Some(credentials) = credentials {
        write(&mut stream, &credentials)?;
        session::authentication(&read(&mut stream, Frame::AuthReply)?)?;
    }
    write(
        &mut stream,
        &Request {
            version: VERSION,
            reserved: 0,
            command: Command::Connect,
            destination,
        },
    )?;
    let response: Reply = read(&mut stream, Frame::Command)?;
    session::reply(&response)?;
    Ok((stream, response.bound))
}

/// Connect to a numeric proxy address and negotiate within one absolute timeout.
/// Returned TCP streams have their handshake timeouts cleared.
///
/// # Errors
/// Returns a terminal connection/session error, including an invalid timeout.
pub fn connect_tcp(
    proxy: SocketAddr,
    destination: Endpoint,
    auth: ClientAuth<'_>,
    timeout: Duration,
) -> Result<(TcpStream, Endpoint), Error> {
    session::endpoint(&destination)?;
    let credentials = auth.credentials()?;
    let deadline = deadline(timeout)?;
    let stream = TcpStream::connect_timeout(&proxy, remaining(deadline)?)?;
    let (stream, bound) = connect_prepared(
        Deadline { stream, deadline },
        destination,
        auth.method(),
        credentials,
    )?;
    stream.stream.set_read_timeout(None)?;
    stream.stream.set_write_timeout(None)?;
    Ok((stream.stream, bound))
}

/// An authenticated CONNECT request awaiting the caller's authorization and dial.
/// Dropping it closes an owned transport without acknowledging success.
pub struct Incoming<S> {
    stream: S,
    destination: Endpoint,
}

impl<S: Read + Write> Incoming<S> {
    /// The original destination, including unresolved domain bytes.
    pub fn destination(&self) -> &Endpoint {
        &self.destination
    }

    /// Send success only after connecting to the target, using its local address
    /// as `bound`, then return the transport with all tunnel bytes preserved.
    ///
    /// # Errors
    /// Returns a codec or transport error; the session must not be reused.
    pub fn accept(mut self, bound: Endpoint) -> Result<S, Error> {
        session::endpoint(&bound)?;
        write(
            &mut self.stream,
            &session::response(ReplyCode::Succeeded, bound),
        )?;
        Ok(self.stream)
    }

    /// Send a failing reply and drop the transport. A success code is rejected.
    ///
    /// # Errors
    /// Returns an I/O/codec error or rejects an attempted success reply.
    pub fn reject(mut self, code: ReplyCode) -> Result<(), Error> {
        if u8::from(code) == 0 {
            return Err(Error::Reply(code));
        }
        write(&mut self.stream, &session::failure(code))
    }
}

/// Authenticate and read one CONNECT request; does not dial or authorize a target.
/// Generic transports need caller-enforced deadlines. No credential data is logged.
///
/// # Errors
/// Failures are terminal. Unsupported commands/address types receive their SOCKS
/// failure reply; malformed greetings close without continuing negotiation.
pub fn accept<S: Read + Write>(mut stream: S, auth: &ServerAuth) -> Result<Incoming<S>, Error> {
    let offer: MethodRequest = read(&mut stream, Frame::Offer)?;
    let selected = auth.select(&offer);
    write(
        &mut stream,
        &MethodSelection {
            version: VERSION,
            method: selected
                .as_ref()
                .copied()
                .unwrap_or(crate::v5::AuthMethod::NoAcceptable),
        },
    )?;
    selected?;
    if auth.method() == crate::v5::AuthMethod::UsernamePassword {
        let credentials: UsernamePasswordRequest = read(&mut stream, Frame::Credentials)?;
        let accepted = auth.authenticate(&credentials);
        write(
            &mut stream,
            &UsernamePasswordResponse {
                version: 1,
                status: UsernamePasswordStatus::from(u8::from(!accepted)),
            },
        )?;
        if !accepted {
            return Err(Error::AuthenticationRejected);
        }
    }
    let result = read::<_, Request>(&mut stream, Frame::Command).and_then(|request| {
        session::request(&request)?;
        Ok(request)
    });
    match result {
        Ok(request) => Ok(Incoming {
            stream,
            destination: request.destination,
        }),
        Err(error) => {
            write(&mut stream, &session::failure(error.reply_code()))?;
            Err(error)
        }
    }
}

fn deadline(timeout: Duration) -> Result<Instant, Error> {
    if timeout.is_zero() {
        return Err(Error::InvalidLimits);
    }
    Instant::now()
        .checked_add(timeout)
        .ok_or(Error::InvalidLimits)
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(session::timed_out)
}

struct Deadline {
    stream: TcpStream,
    deadline: Instant,
}

impl Read for Deadline {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.stream
            .set_read_timeout(Some(remaining(self.deadline)?))?;
        self.stream.read(bytes)
    }
}

impl Write for Deadline {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.stream
            .set_write_timeout(Some(remaining(self.deadline)?))?;
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

fn resolve(destination: &Endpoint) -> Result<Vec<SocketAddr>, Error> {
    match destination {
        Endpoint::Ipv4 { address, port } => Ok(vec![SocketAddr::from((*address, *port))]),
        Endpoint::Ipv6 { address, port } => Ok(vec![SocketAddr::from((*address, *port))]),
        Endpoint::Domain { name, port } => Ok((session::domain_name(name)?, *port)
            .to_socket_addrs()?
            .collect()),
    }
}

fn dial(
    destination: &Endpoint,
    peer: SocketAddr,
    config: &ServerConfig,
) -> Result<TcpStream, Error> {
    // System DNS is synchronous and cannot be interrupted; it is outside the dial budget.
    let addresses = resolve(destination)?;
    let until = deadline(config.limits.connect)?;
    let mut error = if addresses.is_empty() {
        Error::InvalidEndpoint
    } else {
        Error::PermissionDenied
    };
    for address in addresses {
        if config.permits(peer, destination, address) {
            match TcpStream::connect_timeout(&address, remaining(until)?) {
                Ok(stream) => return Ok(stream),
                Err(cause) => error = cause.into(),
            }
        }
    }
    Err(error)
}

fn copy_half(mut source: Deadline, mut target: Deadline) -> io::Result<u64> {
    let result = io::copy(&mut source, &mut target);
    if result.is_err() {
        let _ = source.stream.shutdown(Shutdown::Both);
        let _ = target.stream.shutdown(Shutdown::Both);
    } else {
        target.stream.shutdown(Shutdown::Write)?;
    }
    result
}

fn relay(client: TcpStream, target: TcpStream, timeout: Duration) -> Result<(), Error> {
    let until = deadline(timeout)?;
    let client_read = client.try_clone()?;
    let target_write = target.try_clone()?;
    thread::scope(|scope| {
        let outgoing = thread::Builder::new().spawn_scoped(scope, || {
            copy_half(
                Deadline {
                    stream: client_read,
                    deadline: until,
                },
                Deadline {
                    stream: target_write,
                    deadline: until,
                },
            )
        })?;
        let incoming = copy_half(
            Deadline {
                stream: target,
                deadline: until,
            },
            Deadline {
                stream: client,
                deadline: until,
            },
        );
        let outgoing = outgoing.join().map_err(|_| Error::WorkerPanicked)?;
        outgoing?;
        incoming?;
        Ok(())
    })
}

/// Run one complete TCP proxy connection: authenticate, authorize each resolved
/// target, dial, acknowledge the actual bound address, and relay both directions.
/// EOF half-closes only the opposite write side; an error terminates both halves.
/// Blocking DNS and caller callbacks cannot be interrupted by the I/O deadlines.
///
/// # Errors
/// Returns a terminal session, target, timeout, or relay error.
pub fn serve_connection(stream: TcpStream, config: &ServerConfig) -> Result<(), Error> {
    serve_connection_with(stream, config, |_| Ok(()))
}

fn serve_connection_with(
    stream: TcpStream,
    config: &ServerConfig,
    register_target: impl FnOnce(&TcpStream) -> Result<(), Error>,
) -> Result<(), Error> {
    config.limits.validate()?;
    let peer = stream.peer_addr()?;
    let mut incoming = accept(
        Deadline {
            stream,
            deadline: deadline(config.limits.handshake)?,
        },
        &config.auth,
    )?;
    let target = dial(incoming.destination(), peer, config);
    // Replies get a fresh bounded budget even if a target attempt exhausted its deadline.
    incoming.stream.deadline = deadline(config.limits.handshake.min(Duration::from_secs(10)))?;
    match target {
        Ok(target) => {
            register_target(&target)?;
            let bound = target.local_addr()?.into();
            let client = incoming.accept(bound)?.stream;
            relay(client, target, config.limits.relay)
        }
        Err(error) => {
            incoming.reject(error.reply_code())?;
            Err(error)
        }
    }
}

// Listener shutdown must close both relay sockets. The cancellation bit also
// covers shutdown racing a connect that has not registered its target yet.
struct Sockets {
    client: TcpStream,
    target: Option<TcpStream>,
    cancelled: bool,
}

impl Sockets {
    fn register(&mut self, target: &TcpStream) -> Result<(), Error> {
        if self.cancelled {
            target.shutdown(Shutdown::Both)?;
            return Err(io::Error::from(io::ErrorKind::Interrupted).into());
        }
        self.target = Some(target.try_clone()?);
        Ok(())
    }

    fn shutdown(&mut self) {
        self.cancelled = true;
        let _ = self.client.shutdown(Shutdown::Both);
        if let Some(target) = &self.target {
            let _ = target.shutdown(Shutdown::Both);
        }
    }
}

/// Serve an owned TCP listener with bounded thread-per-session concurrency.
/// Set `shutdown` to stop acceptance and close both active relay sockets, then join
/// workers. Blocking DNS/callbacks and an in-progress bounded connection attempt
/// may delay shutdown. The listener is made
/// nonblocking; its socket options are not restored because it is consumed.
/// `on_error` reports individual session failures without stopping the listener.
/// It must not panic. Saturation leaves new connections in the OS backlog.
///
/// # Errors
/// Returns invalid limits, listener/thread creation errors, or a worker panic.
pub fn serve(
    listener: TcpListener,
    config: &ServerConfig,
    shutdown: &AtomicBool,
    on_error: impl Fn(Error),
) -> Result<(), Error> {
    config.limits.validate()?;
    listener.set_nonblocking(true)?;
    let mut workers = Vec::<(Arc<Mutex<Sockets>>, thread::JoinHandle<Result<(), Error>>)>::new();
    let result = (|| {
        while !shutdown.load(Ordering::Acquire) {
            let mut index = 0;
            while index < workers.len() {
                if workers[index].1.is_finished() {
                    let (_, worker) = workers.swap_remove(index);
                    if let Err(error) = worker.join().map_err(|_| Error::WorkerPanicked)? {
                        on_error(error);
                    }
                } else {
                    index += 1;
                }
            }
            if workers.len() < config.limits.connections {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false)?;
                        let control = Arc::new(Mutex::new(Sockets {
                            client: stream.try_clone()?,
                            target: None,
                            cancelled: false,
                        }));
                        let registration = Arc::clone(&control);
                        let config = config.clone();
                        let worker = thread::Builder::new().spawn(move || {
                            serve_connection_with(stream, &config, |target| {
                                registration
                                    .lock()
                                    .map_err(|_| Error::WorkerPanicked)?
                                    .register(target)
                            })
                        })?;
                        workers.push((control, worker));
                        continue;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            thread::park_timeout(Duration::from_millis(10));
        }
        Ok(())
    })();
    drop(listener);
    let mut result = result;
    for (control, _) in &workers {
        match control.lock() {
            Ok(mut sockets) => sockets.shutdown(),
            Err(_) => result = Err(Error::WorkerPanicked),
        }
    }
    for (_, worker) in workers {
        match worker.join() {
            Ok(Err(error)) => on_error(error),
            Err(_) => result = Err(Error::WorkerPanicked),
            Ok(Ok(())) => {}
        }
    }
    result
}
