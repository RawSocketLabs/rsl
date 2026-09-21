//! Blocking SOCKS5 client handshakes. Generic transports have caller-managed deadlines.
use crate::io::blocking::{Deadline, deadline, remaining, write};
use crate::v5::{
    Command, Endpoint, MethodRequest, MethodSelection, Reply, Request as WireRequest,
    UsernamePasswordRequest, UsernamePasswordResponse, VERSION,
};
use crate::{
    Stream,
    error::Error,
    v5::{auth::ClientAuth, decode},
};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
/// Negotiate one CONNECT request and return a lossless tunnel and
/// the proxy's bound endpoint. Domains are sent to the proxy, not resolved locally.
///
/// # Errors
/// Returns a terminal I/O, codec, authentication, or peer-reply error. Invalid
/// local arguments fail before writing any handshake bytes.
pub fn connect<S: Read + Write>(
    stream: S,
    destination: Endpoint,
    auth: ClientAuth<'_>,
) -> Result<(Stream<S>, Endpoint), Error> {
    destination.validate_connect_destination()?;
    let credentials = auth.credentials()?;
    connect_prepared(stream, destination, auth.method(), credentials)
}

fn connect_prepared<S: Read + Write>(
    stream: S,
    destination: Endpoint,
    method: crate::v5::AuthMethod,
    credentials: Option<UsernamePasswordRequest>,
) -> Result<(Stream<S>, Endpoint), Error> {
    let mut stream = Stream::new(stream);
    write(
        &mut stream,
        &MethodRequest {
            version: VERSION,
            methods: vec![method],
        },
    )?;
    let selected: MethodSelection = stream.read_message()?;
    selected.check_offered(method)?;
    if let Some(credentials) = credentials {
        write(&mut stream, &credentials)?;
        stream
            .read_message::<UsernamePasswordResponse>()?
            .ensure_success()?;
    }
    write(
        &mut stream,
        &WireRequest {
            version: VERSION,
            reserved: 0,
            command: Command::Connect,
            destination,
        },
    )?;
    let response: Reply = stream
        .read_message()
        .map_err(|error| decode::command_error(error, &mut stream.buffered))?;
    response.ensure_success()?;
    Ok((stream, response.bound))
}

/// Connect to a numeric proxy address and negotiate within one absolute timeout.
/// The returned tunnel's TCP transport has its handshake timeouts cleared.
///
/// # Errors
/// Returns a terminal connection/session error, including an invalid timeout.
pub fn connect_tcp(
    proxy: SocketAddr,
    destination: Endpoint,
    auth: ClientAuth<'_>,
    timeout: Duration,
) -> Result<(Stream<TcpStream>, Endpoint), Error> {
    destination.validate_connect_destination()?;
    let credentials = auth.credentials()?;
    let deadline = deadline(timeout)?;
    let stream = TcpStream::connect_timeout(&proxy, remaining(deadline)?)?;
    let (stream, bound) = connect_prepared(
        Deadline { stream, deadline },
        destination,
        auth.method(),
        credentials,
    )?;
    let (stream, buffered) = stream.into_parts();
    stream.stream.set_read_timeout(None)?;
    stream.stream.set_write_timeout(None)?;
    Ok((Stream::from_parts(stream.stream, buffered), bound))
}
