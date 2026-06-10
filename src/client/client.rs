use std::net::{SocketAddr, TcpStream};

use binrw::{io::NoSeek, BinWrite};

use crate::auth::{AuthHandler, NoAuth, UserPassHandler};
use crate::client::bind::BindListener;
use crate::client::udp::UdpTunnel;
use crate::error::{Result, SocksError};
use crate::v5::{
    Address, Command, IdentifierBuilder, Method, Offer, Reply, RequestBuilder, Response,
};

/// A connection target: a socket address, or a domain name resolved by the
/// proxy.
#[derive(Clone, Debug, PartialEq)]
pub enum TargetAddr {
    Ip(SocketAddr),
    Domain(String, u16),
}

impl TargetAddr {
    /// Converts the target into a wire [`Address`].
    ///
    /// # Errors
    /// Returns [`SocksError::Validation`] when a domain exceeds the 255-byte
    /// wire limit.
    pub(crate) fn address(&self) -> Result<Address> {
        match self {
            TargetAddr::Ip(addr) => Ok(Address::from(*addr)),
            TargetAddr::Domain(domain, _) => {
                if domain.len() > 255 {
                    return Err(SocksError::Validation(
                        "domain exceeds 255 bytes".to_string(),
                    ));
                }
                Ok(Address::Domain(domain.as_str().into()))
            }
        }
    }

    pub(crate) fn port(&self) -> u16 {
        match self {
            TargetAddr::Ip(addr) => addr.port(),
            TargetAddr::Domain(_, port) => *port,
        }
    }
}

impl From<SocketAddr> for TargetAddr {
    fn from(addr: SocketAddr) -> Self {
        TargetAddr::Ip(addr)
    }
}

impl From<(&str, u16)> for TargetAddr {
    fn from((domain, port): (&str, u16)) -> Self {
        TargetAddr::Domain(domain.to_string(), port)
    }
}

impl From<(String, u16)> for TargetAddr {
    fn from((domain, port): (String, u16)) -> Self {
        TargetAddr::Domain(domain, port)
    }
}

/// A SOCKS5 client that negotiates with a proxy and issues commands.
pub struct Client {
    pub proxy: SocketAddr,
    auth: Vec<Box<dyn AuthHandler>>,
}

impl Client {
    /// Creates a client that offers only the no-authentication method.
    pub fn new(proxy: SocketAddr) -> Self {
        Self {
            proxy,
            auth: vec![Box::new(NoAuth)],
        }
    }

    /// Creates a client that offers Username/Password authentication
    /// (RFC 1929) alongside the no-authentication method.
    ///
    /// # Errors
    /// Returns [`SocksError::Validation`] when either credential exceeds the
    /// 255-byte wire limit.
    pub fn with_user_pass(proxy: SocketAddr, username: &str, password: &str) -> Result<Self> {
        Ok(Self {
            proxy,
            auth: vec![
                Box::new(UserPassHandler::new(username, password)?),
                Box::new(NoAuth),
            ],
        })
    }

    /// Creates a client that offers the given handlers, in preference order.
    pub fn with_auth(proxy: SocketAddr, auth: Vec<Box<dyn AuthHandler>>) -> Self {
        Self { proxy, auth }
    }

    /// Connects to the proxy and completes method negotiation, returning the
    /// authenticated stream.
    pub(crate) fn negotiate(&self) -> Result<TcpStream> {
        //~ implements rfc1928#3/must.d9ac89
        let mut stream = TcpStream::connect(self.proxy)?;

        let identifier = IdentifierBuilder::default()
            .methods(self.auth.iter().map(|handler| handler.method()).collect())
            .build()
            .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;
        identifier.write(&mut NoSeek::new(&stream))?;

        let offer = Offer::read_from(&mut &stream)?;
        if offer.version != 5 {
            return Err(SocksError::UnsupportedVersion(offer.version));
        }
        // X'FF' means none of the offered methods is acceptable; returning
        // here drops the stream, closing the connection.
        //~ implements rfc1928#3/must.bfe149
        if offer.method == Method::NoAcceptableMethods {
            return Err(SocksError::NoAcceptableMethods);
        }

        let handler = self
            .auth
            .iter()
            .find(|handler| handler.method() == offer.method)
            .ok_or(SocksError::NoAcceptableMethods)?;
        handler.negotiate(&mut stream)?;

        Ok(stream)
    }

    /// Sends a command request over a freshly negotiated stream and returns
    /// the stream together with the first successful reply.
    pub(crate) fn request(
        &self,
        command: Command,
        target: &TargetAddr,
    ) -> Result<(TcpStream, Reply)> {
        let stream = self.negotiate()?;

        let address = target.address()?;
        let request = RequestBuilder::default()
            .command(command)
            .address_type(address.address_type())
            .dest_addr(address)
            .dest_port(target.port())
            .build()
            .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;
        request.write(&mut NoSeek::new(&stream))?;

        let reply = read_reply(&stream)?;
        Ok((stream, reply))
    }

    /// Establishes a CONNECT tunnel to `target`, returning a stream that is
    /// transparent to the remote host.
    ///
    /// # Errors
    /// Returns [`SocksError::ReplyFailure`] when the proxy refuses the
    /// connection, or an I/O, negotiation, or parse error.
    pub fn connect(&self, target: impl Into<TargetAddr>) -> Result<TcpStream> {
        let (stream, _) = self.request(Command::Connect, &target.into())?;
        Ok(stream)
    }

    /// Asks the proxy to listen for an inbound connection from
    /// `expected_peer` (RFC 1928 BIND).
    ///
    /// # Errors
    /// Returns [`SocksError::ReplyFailure`] when the proxy refuses the bind,
    /// or an I/O, negotiation, or parse error.
    pub fn bind(&self, expected_peer: impl Into<TargetAddr>) -> Result<BindListener> {
        let (stream, reply) = self.request(Command::Bind, &expected_peer.into())?;
        let bound = reply_socket_addr(&reply)?;
        Ok(BindListener::new(stream, bound))
    }

    /// Establishes a UDP ASSOCIATE relay (RFC 1928 section 7).
    ///
    /// # Errors
    /// Returns [`SocksError::ReplyFailure`] when the proxy refuses the
    /// association, or an I/O, negotiation, or parse error.
    pub fn udp_associate(&self) -> Result<UdpTunnel> {
        UdpTunnel::establish(self)
    }
}

/// Reads a reply and verifies version and status.
pub(crate) fn read_reply(stream: &TcpStream) -> Result<Reply> {
    let reply = Reply::read_from(&mut &*stream)?;
    if reply.version != 5 {
        return Err(SocksError::UnsupportedVersion(reply.version));
    }
    if reply.reply != Response::Succeeded {
        return Err(SocksError::ReplyFailure(reply.reply));
    }
    Ok(reply)
}

/// Extracts the bound socket address from a reply.
pub(crate) fn reply_socket_addr(reply: &Reply) -> Result<SocketAddr> {
    match reply.bind_addr {
        Address::V4(addr) => Ok(SocketAddr::from((addr, reply.bind_port))),
        Address::V6(addr) => Ok(SocketAddr::from((addr, reply.bind_port))),
        Address::Domain(_) => Err(SocksError::NotSupported(
            "domain bind address in reply".to_string(),
        )),
    }
}
