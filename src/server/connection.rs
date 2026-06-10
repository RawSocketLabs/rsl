use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use binrw::{io::NoSeek, BinWrite};

use crate::auth::Authenticator;
use crate::error::{Result, SocksError};
use crate::server::relay::relay;
use crate::server::udp::handle_udp_associate;
use crate::v5::{
    Address, Command, Identifier, Method, OfferBuilder, ReplyBuilder, Request, Response,
};

/// Serves a single client connection: method negotiation, authentication,
/// command dispatch, and relaying.
///
/// `handshake_timeout`, when set, bounds the time the client has to complete
/// negotiation, authentication, and the request — a stalled client cannot
/// hold a handler open. It is cleared before an established relay begins, so
/// long-lived idle tunnels are unaffected.
pub(crate) fn handle_client(
    mut stream: TcpStream,
    authenticators: &[Box<dyn Authenticator>],
    handshake_timeout: Option<Duration>,
    bind_timeout: Option<Duration>,
) -> Result<()> {
    if let Some(timeout) = handshake_timeout {
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
    }

    let identifier = Identifier::read_from(&mut &stream)?;
    if identifier.version != 5 {
        return Err(SocksError::UnsupportedVersion(identifier.version));
    }

    let selected = authenticators
        .iter()
        .find(|authenticator| identifier.methods.contains(&authenticator.method()));

    let Some(authenticator) = selected else {
        let offer = OfferBuilder::default()
            .method(Method::NoAcceptableMethods)
            .build()
            .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;
        offer.write(&mut NoSeek::new(&stream))?;
        return Err(SocksError::NoAcceptableMethods);
    };

    let offer = OfferBuilder::default()
        .method(authenticator.method())
        .build()
        .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;
    offer.write(&mut NoSeek::new(&stream))?;

    authenticator.authenticate(&mut stream)?;

    let request = Request::read_from(&mut &stream)?;
    if request.version != 5 {
        return Err(SocksError::UnsupportedVersion(request.version));
    }

    // The handshake is complete; an established relay may idle indefinitely,
    // so lift the negotiation deadline before handing off.
    if handshake_timeout.is_some() {
        stream.set_read_timeout(None)?;
        stream.set_write_timeout(None)?;
    }

    match request.command {
        Command::Connect => handle_connect(stream, &request),
        Command::Bind => handle_bind(stream, &request, bind_timeout),
        Command::UdpAssociate => handle_udp_associate(stream, &request),
        Command::Custom(other) => {
            send_failure(&stream, Response::CommandNotSupported)?;
            Err(SocksError::NotSupported(format!("command {:#04x}", other)))
        }
    }
}

/// Sends a reply carrying the given response code and bound address.
///
/// RSV defaults to X'00' in [`ReplyBuilder`], as required.
//~ implements rfc1928#6/must.66f64a part="reply RSV field"
pub(crate) fn send_reply(stream: &TcpStream, response: Response, addr: SocketAddr) -> Result<()> {
    let bind_addr = Address::from(addr);
    let reply = ReplyBuilder::default()
        .reply(response)
        .address_type(bind_addr.address_type())
        .bind_addr(bind_addr)
        .bind_port(addr.port())
        .build()
        .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;
    reply.write(&mut NoSeek::new(stream))?;
    Ok(())
}

/// Sends a failure reply with an unspecified bound address.
///
/// Every caller returns the propagating error immediately afterward, dropping
/// the stream and terminating the TCP connection well within the 10-second
/// bound the RFC sets.
//~ implements rfc1928#6/must.9c2599
//~ implements rfc1928#6/must.0bc643
pub(crate) fn send_failure(stream: &TcpStream, response: Response) -> Result<()> {
    send_reply(
        stream,
        response,
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
    )
}

/// Maps a connection error to the closest SOCKS5 reply code.
fn response_for(err: &io::Error) -> Response {
    match err.kind() {
        io::ErrorKind::ConnectionRefused => Response::ConnectionRefused,
        io::ErrorKind::TimedOut | io::ErrorKind::NotFound => Response::HostUnreachable,
        _ => Response::GeneralFailure,
    }
}

fn handle_connect(stream: TcpStream, request: &Request) -> Result<()> {
    let target = request.dest_addr.to_socket_string(request.dest_port);
    let conn = match TcpStream::connect(&target) {
        Ok(conn) => conn,
        Err(err) => {
            send_failure(&stream, response_for(&err))?;
            return Err(err.into());
        }
    };

    send_reply(&stream, Response::Succeeded, conn.local_addr()?)?;
    relay(stream, conn)
}

/// Accept one connection, optionally bounded by a deadline. Without a timeout
/// it blocks; with one it polls the non-blocking listener until a peer arrives
/// or the deadline passes, returning a `TimedOut` error so a BIND whose peer
/// never connects cannot hold the handler (and its listener) open forever.
fn accept_with_timeout(
    listener: &TcpListener,
    timeout: Option<Duration>,
) -> io::Result<(TcpStream, SocketAddr)> {
    let Some(timeout) = timeout else {
        return listener.accept();
    };
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + timeout;
    let result = loop {
        match listener.accept() {
            Ok(accepted) => break Ok(accepted),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    break Err(io::Error::new(io::ErrorKind::TimedOut, "BIND peer-wait timed out"));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => break Err(err),
        }
    };
    // Restore blocking mode so the relay then reads normally.
    if let Ok((conn, _)) = &result {
        conn.set_nonblocking(false)?;
    }
    result
}

fn handle_bind(stream: TcpStream, request: &Request, bind_timeout: Option<Duration>) -> Result<()> {
    let listener = match TcpListener::bind((stream.local_addr()?.ip(), 0)) {
        Ok(listener) => listener,
        Err(err) => {
            send_failure(&stream, Response::GeneralFailure)?;
            return Err(err.into());
        }
    };

    send_reply(&stream, Response::Succeeded, listener.local_addr()?)?;

    let (conn, peer) = match accept_with_timeout(&listener, bind_timeout) {
        Ok(accepted) => accepted,
        Err(err) if err.kind() == io::ErrorKind::TimedOut => {
            send_failure(&stream, Response::TtlExpired)?;
            return Err(err.into());
        }
        Err(err) => {
            send_failure(&stream, Response::GeneralFailure)?;
            return Err(err.into());
        }
    };

    // RFC 1928 leaves peer verification optional; enforce it only when the
    // request named a concrete address.
    let expected = match request.dest_addr {
        Address::V4(addr) if !addr.is_unspecified() => Some(IpAddr::V4(addr)),
        Address::V6(addr) if !addr.is_unspecified() => Some(IpAddr::V6(addr)),
        _ => None,
    };
    if let Some(expected) = expected {
        if expected != peer.ip() {
            send_failure(&stream, Response::ConnectionNotAllowed)?;
            return Err(SocksError::Validation(format!(
                "unexpected bind peer: {}",
                peer
            )));
        }
    }

    send_reply(&stream, Response::Succeeded, peer)?;
    relay(stream, conn)
}
