//! Blocking SOCKS5 client handshakes. Generic transports have caller-managed deadlines.
// --- Standard library ---
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

// --- Internal modules ---
use super::exchange::exchange;
use crate::io::blocking::{Deadline, deadline, remaining};
use crate::v5::{Command, Endpoint};
use crate::{Stream, error::Error, v5::auth::ClientAuth};
/// Negotiate one CONNECT request and return a lossless tunnel and
/// the proxy's bound endpoint. Domains are sent to the proxy, not resolved locally.
///
/// # Errors
/// Returns a terminal I/O, codec, authentication, or peer-reply error. Invalid
/// local arguments fail before writing any handshake bytes.
pub fn connect_with<S: Read + Write>(
    stream: S,
    dest: Endpoint,
    auth: ClientAuth<'_>,
) -> Result<(Stream<S>, Endpoint), Error> {
    dest.validate_destination()?;
    let credentials = auth.credentials()?;
    exchange(stream, dest, Command::Connect, auth.method(), credentials)
}

/// Connect to a numeric proxy address and negotiate within one absolute timeout.
/// The returned tunnel's TCP transport has its handshake timeouts cleared.
///
/// # Errors
/// Returns a terminal connection/session error, including an invalid timeout.
pub fn connect(
    proxy: SocketAddr,
    dest: Endpoint,
    auth: ClientAuth<'_>,
    timeout: Duration,
) -> Result<(Stream<TcpStream>, Endpoint), Error> {
    dest.validate_destination()?;
    let credentials = auth.credentials()?;
    let deadline = deadline(timeout)?;
    let stream = TcpStream::connect_timeout(&proxy, remaining(deadline)?)?;
    let (stream, bound) = exchange(
        Deadline { stream, deadline },
        dest,
        Command::Connect,
        auth.method(),
        credentials,
    )?;
    let (stream, buffered) = stream.into_parts();
    stream.stream.set_read_timeout(None)?;
    stream.stream.set_write_timeout(None)?;
    Ok((Stream::from_parts(stream.stream, buffered), bound))
}
