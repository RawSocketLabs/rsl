//! Raw SOCKS4 / 4A client transport for testing and research.
//!
//! [`RawClient`] is the deliberately *non-compliant* counterpart to
//! [`Client`](super::Client): it sets no defaults and validates nothing. It
//! writes exactly the bytes or wire messages it is given and hands back
//! whatever the peer returns, so callers can probe how a SOCKS4 implementation
//! responds to malformed or hostile frames (fuzzing, interop testing, security
//! research). The compliant path lives in [`Client`](super::Client); the
//! [`malformed`] module provides ready-made spec-violating frames.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use binrw::{BinWrite, io::NoSeek};

use crate::error::Result;
use crate::v4::{Reply, Request};

/// A validation-free SOCKS4 transport over a TCP stream.
pub struct RawClient {
    stream: TcpStream,
}

impl RawClient {
    /// Connects to a proxy without sending anything.
    ///
    /// # Errors
    /// Returns an error if the connection cannot be established.
    pub fn connect(proxy: impl ToSocketAddrs) -> Result<Self> {
        Ok(Self {
            stream: TcpStream::connect(proxy)?,
        })
    }

    /// Wraps an already-open stream.
    pub fn from_stream(stream: TcpStream) -> Self {
        Self { stream }
    }

    /// The underlying stream, for direct manipulation.
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }

    /// Sets a read timeout so a non-responsive peer cannot block reads forever.
    ///
    /// # Errors
    /// Returns an error if the timeout cannot be applied.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        Ok(self.stream.set_read_timeout(timeout)?)
    }

    /// Writes raw bytes verbatim — no framing, no validation.
    ///
    /// # Errors
    /// Returns an error if the write fails.
    pub fn write_bytes(&self, bytes: &[u8]) -> Result<()> {
        (&self.stream).write_all(bytes)?;
        Ok(())
    }

    /// Writes a request exactly as given — version, command, and address are
    /// not validated, and the USERID/DOMAIN terminators are whatever the
    /// [`Request`] encodes.
    ///
    /// # Errors
    /// Returns an error if serialization or the write fails.
    pub fn write_request(&self, request: &Request) -> Result<()> {
        request.write(&mut NoSeek::new(&self.stream))?;
        Ok(())
    }

    /// Reads up to `max` bytes of whatever the peer sent, unparsed.
    ///
    /// # Errors
    /// Returns an error if the read fails.
    pub fn read_bytes(&self, max: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; max];
        let read = (&self.stream).read(&mut buf)?;
        buf.truncate(read);
        Ok(buf)
    }

    /// Reads and parses the server's 8-byte reply.
    ///
    /// # Errors
    /// Returns an error if the read or parse fails.
    pub fn read_reply(&self) -> Result<Reply> {
        Reply::read_from(&mut &self.stream)
    }
}

/// Ready-made frames that intentionally violate the SOCKS4 / 4A format, for
/// exercising a server's handling of malformed input. Each generator is named
/// for the violation it embodies.
pub mod malformed {
    use std::net::Ipv4Addr;

    use binrw::NullString;

    use crate::v4::{Command, Request, RequestBuilder};

    /// A request advertising SOCKS version 5 instead of 4.
    //~ deviates socks4#front/should.8d142d profile="raw" reason="emits VN=5 to test a SOCKS4 server's version handling"
    pub fn wrong_version_request(ip: Ipv4Addr, port: u16) -> Request {
        RequestBuilder::default()
            .version(5)
            .command(Command::Connect)
            .dest_ip(ip)
            .dest_port(port)
            .build()
            .expect("request builds")
    }

    /// A request carrying an unassigned command byte (3).
    pub fn unknown_command_request(ip: Ipv4Addr, port: u16) -> Request {
        RequestBuilder::default()
            .command(Command::Custom(3))
            .dest_ip(ip)
            .dest_port(port)
            .build()
            .expect("request builds")
    }

    /// A request with a non-empty userid, for exercising identd-style checks.
    pub fn request_with_userid(ip: Ipv4Addr, port: u16, userid: &str) -> Request {
        RequestBuilder::default()
            .command(Command::Connect)
            .dest_ip(ip)
            .dest_port(port)
            .userid(NullString::from(userid))
            .build()
            .expect("request builds")
    }
}
