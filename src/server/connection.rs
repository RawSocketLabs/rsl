use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};

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
pub(crate) fn handle_client(
    mut stream: TcpStream,
    authenticators: &[Box<dyn Authenticator>],
) -> Result<()> {
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

    match request.command {
        Command::Connect => handle_connect(stream, &request),
        Command::Bind => handle_bind(stream, &request),
        Command::UdpAssociate => handle_udp_associate(stream, &request),
        Command::Custom(other) => {
            send_failure(&stream, Response::CommandNotSupported)?;
            Err(SocksError::NotSupported(format!("command {:#04x}", other)))
        }
    }
}

/// Sends a reply carrying the given response code and bound address.
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

fn handle_bind(stream: TcpStream, request: &Request) -> Result<()> {
    let listener = match TcpListener::bind((stream.local_addr()?.ip(), 0)) {
        Ok(listener) => listener,
        Err(err) => {
            send_failure(&stream, Response::GeneralFailure)?;
            return Err(err.into());
        }
    };

    send_reply(&stream, Response::Succeeded, listener.local_addr()?)?;

    let (conn, peer) = match listener.accept() {
        Ok(accepted) => accepted,
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
