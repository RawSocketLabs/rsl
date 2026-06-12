//! Raw client transport for testing and research.
//!
//! [`RawClient`] is the deliberately *non-compliant* counterpart to
//! [`Client`](crate::client::Client): it performs no method negotiation, sets
//! no defaults, and validates nothing. It writes exactly the bytes or wire
//! messages it is given and hands back whatever the peer returns, so callers
//! can probe how a SOCKS implementation responds to malformed or hostile
//! frames (fuzzing, interop testing, security research).
//!
//! This is the dual-use "raw" surface of the library — the compliant path
//! lives in [`Client`](crate::client::Client). The [`malformed`] module
//! provides ready-made frames that intentionally violate RFC 1928/1929.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use binrw::{BinWrite, io::NoSeek};

use crate::error::Result;
use crate::v5::{Identifier, Offer, Reply, Request, UserPassRequest, UserPassResponse};

/// A validation-free SOCKS transport over a TCP stream.
pub struct RawClient {
    stream: TcpStream,
}

impl RawClient {
    /// Connects to a proxy without sending anything. No negotiation occurs.
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

    /// Writes raw bytes verbatim — no framing, no validation. Use for
    /// truncated frames, oversized fields, or arbitrary garbage.
    ///
    /// # Errors
    /// Returns an error if the write fails.
    pub fn write_bytes(&self, bytes: &[u8]) -> Result<()> {
        (&self.stream).write_all(bytes)?;
        Ok(())
    }

    /// Writes a method identifier exactly as given. Version, method count, and
    /// methods go out as-is, so a deliberately wrong count or an unknown
    /// method survives to the wire.
    ///
    /// # Errors
    /// Returns an error if serialization or the write fails.
    pub fn write_identifier(&self, identifier: &Identifier) -> Result<()> {
        identifier.write(&mut NoSeek::new(&self.stream))?;
        Ok(())
    }

    /// Writes a command request exactly as given — version, reserved byte,
    /// command, and address are not validated.
    ///
    /// # Errors
    /// Returns an error if serialization or the write fails.
    pub fn write_request(&self, request: &Request) -> Result<()> {
        request.write(&mut NoSeek::new(&self.stream))?;
        Ok(())
    }

    /// Writes a username/password sub-negotiation request exactly as given.
    ///
    /// # Errors
    /// Returns an error if serialization or the write fails.
    pub fn write_user_pass_request(&self, request: &UserPassRequest) -> Result<()> {
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

    /// Reads and parses the server's method-selection offer.
    ///
    /// # Errors
    /// Returns an error if the read or parse fails.
    pub fn read_offer(&self) -> Result<Offer> {
        Offer::read_from(&mut &self.stream)
    }

    /// Reads and parses the server's command reply.
    ///
    /// # Errors
    /// Returns an error if the read or parse fails.
    pub fn read_reply(&self) -> Result<Reply> {
        Reply::read_from(&mut &self.stream)
    }

    /// Reads and parses the server's username/password response.
    ///
    /// # Errors
    /// Returns an error if the read or parse fails.
    pub fn read_user_pass_response(&self) -> Result<UserPassResponse> {
        UserPassResponse::read_from(&mut &self.stream)
    }
}

/// Ready-made frames that intentionally violate RFC 1928/1929, for exercising
/// a server's handling of malformed input. Each generator is named for the
/// violation it embodies; send them with [`RawClient::write_identifier`],
/// [`RawClient::write_request`], or [`RawClient::write_bytes`].
pub mod malformed {
    use std::net::SocketAddr;

    use crate::v5::{
        Address, Command, Identifier, IdentifierBuilder, Method, Request, RequestBuilder,
    };

    /// A method identifier advertising SOCKS version 4 instead of 5.
    pub fn wrong_version_identifier(methods: Vec<Method>) -> Identifier {
        IdentifierBuilder::default()
            .version(4)
            .methods(methods)
            .build()
            .expect("identifier builds")
    }

    /// An identifier whose advertised method count (here 3) exceeds the
    /// methods actually present — a server reading by the count will block or
    /// misframe.
    pub fn overstated_method_count() -> Identifier {
        IdentifierBuilder::default()
            .number_of_methods(3u8)
            .methods(vec![Method::NoAuth])
            .build()
            .expect("identifier builds")
    }

    /// A CONNECT request with a non-zero RESERVED byte. RFC 1928 §6 requires
    /// RSV to be X'00'; this deliberately violates that.
    //~ deviates rfc1928#6/must.66f64a profile="raw" reason="emits a non-zero RSV to test a server's tolerance of malformed frames"
    pub fn non_zero_reserved_request(target: SocketAddr) -> Request {
        let address = Address::from(target);
        RequestBuilder::default()
            .command(Command::Connect)
            .reserved(0xFF)
            .address_type(address.address_type())
            .dest_addr(address)
            .dest_port(target.port())
            .build()
            .expect("request builds")
    }

    /// A request carrying an unassigned command byte (X'09').
    pub fn unknown_command_request(target: SocketAddr) -> Request {
        let address = Address::from(target);
        RequestBuilder::default()
            .command(Command::Custom(0x09))
            .address_type(address.address_type())
            .dest_addr(address)
            .dest_port(target.port())
            .build()
            .expect("request builds")
    }
}

#[cfg(test)]
mod unit {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::error::SocksError;
    use crate::server::Server;
    use crate::v5::{IdentifierBuilder, Method, RequestBuilder, Response};

    fn spawn_proxy() -> (SocketAddr, thread::JoinHandle<crate::error::Result<()>>) {
        let server = Server::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let handle = thread::spawn(move || server.accept());
        (addr, handle)
    }

    fn spawn_echo() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 256];
                while let Ok(n) = stream.read(&mut buf) {
                    if n == 0 || stream.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            }
        });
        addr
    }

    #[test]
    fn raw_client_drives_a_valid_handshake() {
        let echo = spawn_echo();
        let (proxy, server) = spawn_proxy();

        let raw = RawClient::connect(proxy).unwrap();
        let id = IdentifierBuilder::default()
            .methods(vec![Method::NoAuth])
            .build()
            .unwrap();
        raw.write_identifier(&id).unwrap();
        let offer = raw.read_offer().unwrap();
        assert_eq!(offer.method, Method::NoAuth);

        let address = crate::v5::Address::from(echo);
        let request = RequestBuilder::default()
            .command(crate::v5::Command::Connect)
            .address_type(address.address_type())
            .dest_addr(address)
            .dest_port(echo.port())
            .build()
            .unwrap();
        raw.write_request(&request).unwrap();
        let reply = raw.read_reply().unwrap();
        assert_eq!(reply.reply, Response::Succeeded);

        raw.write_bytes(b"ping").unwrap();
        let echoed = raw.read_bytes(4).unwrap();
        assert_eq!(&echoed, b"ping");

        drop(raw);
        assert!(server.join().unwrap().is_ok());
    }

    #[test]
    fn server_rejects_wrong_version() {
        let (proxy, server) = spawn_proxy();
        let raw = RawClient::connect(proxy).unwrap();
        raw.write_identifier(&malformed::wrong_version_identifier(vec![Method::NoAuth]))
            .unwrap();
        assert!(matches!(
            server.join().unwrap(),
            Err(SocksError::UnsupportedVersion(4))
        ));
    }

    #[test]
    fn server_replies_command_not_supported_to_unknown_command() {
        let (proxy, server) = spawn_proxy();
        let raw = RawClient::connect(proxy).unwrap();
        raw.write_identifier(
            &IdentifierBuilder::default()
                .methods(vec![Method::NoAuth])
                .build()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(raw.read_offer().unwrap().method, Method::NoAuth);

        let target = SocketAddr::from(([127, 0, 0, 1], 9));
        raw.write_request(&malformed::unknown_command_request(target))
            .unwrap();
        let reply = raw.read_reply().unwrap();
        assert_eq!(reply.reply, Response::CommandNotSupported);

        let _ = server.join().unwrap(); // server returns the NotSupported error
    }

    #[test]
    fn server_tolerates_non_zero_reserved() {
        // RSV!=0 violates the spec on send, but a server should be liberal in
        // what it accepts: ours processes the request normally.
        let echo = spawn_echo();
        let (proxy, server) = spawn_proxy();
        let raw = RawClient::connect(proxy).unwrap();
        raw.write_identifier(
            &IdentifierBuilder::default()
                .methods(vec![Method::NoAuth])
                .build()
                .unwrap(),
        )
        .unwrap();
        raw.read_offer().unwrap();

        raw.write_request(&malformed::non_zero_reserved_request(echo))
            .unwrap();
        let reply = raw.read_reply().unwrap();
        assert_eq!(reply.reply, Response::Succeeded);

        drop(raw);
        assert!(server.join().unwrap().is_ok());
    }

    #[test]
    fn overstated_method_count_stalls_until_timeout() {
        let server = Server::bind("127.0.0.1:0")
            .unwrap()
            .with_handshake_timeout(Some(Duration::from_millis(200)));
        let addr = server.local_addr().unwrap();
        let handle = thread::spawn(move || server.accept());

        let raw = RawClient::connect(addr).unwrap();
        // Advertises 3 methods but sends 1; the server waits for the missing
        // bytes and the handshake deadline fires.
        raw.write_identifier(&malformed::overstated_method_count())
            .unwrap();
        assert!(handle.join().unwrap().is_err());
    }
}
