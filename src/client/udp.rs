use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};

use binrw::{io::Cursor, BinRead, BinWrite};

use crate::client::client::{reply_socket_addr, Client, TargetAddr};
use crate::error::{Result, SocksError};
use crate::v5::{Address, Command, UdpHeader, UdpHeaderBuilder};

/// Maximum UDP request header size: reserved + frag + address type +
/// length-prefixed domain + port.
const MAX_UDP_HEADER: usize = 2 + 1 + 1 + 256 + 2;

/// An established UDP ASSOCIATE relay (RFC 1928 section 7).
///
/// Datagrams are wrapped in a [`UdpHeader`] and exchanged with the proxy's
/// relay. Dropping the tunnel closes the TCP control connection, which
/// terminates the association.
pub struct UdpTunnel {
    _control: TcpStream,
    socket: UdpSocket,
    relay: SocketAddr,
}

impl UdpTunnel {
    pub(crate) fn establish(client: &Client) -> Result<UdpTunnel> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        let local = socket.local_addr()?;

        // The client knows its UDP port but not its externally-visible
        // address, so it advertises the port with an all-zeros address.
        //~ implements rfc1928#6/must.ff014a
        let (control, reply) = client.request(
            Command::UdpAssociate,
            &TargetAddr::Ip(SocketAddr::from((Ipv4Addr::UNSPECIFIED, local.port()))),
        )?;

        let mut relay = reply_socket_addr(&reply)?;
        if relay.ip().is_unspecified() {
            relay.set_ip(client.proxy.ip());
        }

        Ok(Self {
            _control: control,
            socket,
            relay,
        })
    }

    /// Sends `payload` to `target` through the relay.
    ///
    /// # Errors
    /// Returns an error if the header cannot be constructed or the datagram
    /// cannot be sent.
    pub fn send_to(&self, target: impl Into<TargetAddr>, payload: &[u8]) -> Result<()> {
        let target = target.into();
        let address = target.address()?;

        let header = UdpHeaderBuilder::default()
            .address_type(address.address_type())
            .dest_addr(address)
            .dest_port(target.port())
            .build()
            .map_err(|err| SocksError::MessageConstruction(err.to_string()))?;

        let mut cursor = Cursor::new(Vec::new());
        header.write(&mut cursor)?;

        let mut datagram = cursor.into_inner();
        datagram.extend_from_slice(payload);
        // `self.relay` is BND.ADDR/BND.PORT from the UDP ASSOCIATE reply.
        //~ implements rfc1928#7/must.784a05
        self.socket.send_to(&datagram, self.relay)?;

        Ok(())
    }

    /// Receives a datagram from the relay, copying the payload into `buf` and
    /// returning its length and the originating target.
    ///
    /// # Errors
    /// Returns [`SocksError::NotSupported`] for fragmented datagrams, or an
    /// I/O or parse error.
    pub fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, TargetAddr)> {
        let mut datagram = vec![0u8; MAX_UDP_HEADER + buf.len()];
        let (received, _) = self.socket.recv_from(&mut datagram)?;

        let mut cursor = Cursor::new(&datagram[..received]);
        let header = UdpHeader::read(&mut cursor)?;
        if header.frag != 0 {
            return Err(SocksError::NotSupported(
                "fragmented UDP datagrams".to_string(),
            ));
        }

        let source = match header.dest_addr {
            Address::V4(addr) => TargetAddr::Ip(SocketAddr::from((addr, header.dest_port))),
            Address::V6(addr) => TargetAddr::Ip(SocketAddr::from((addr, header.dest_port))),
            Address::Domain(ref domain) => TargetAddr::Domain(domain.to_string(), header.dest_port),
        };

        let start = cursor.position() as usize;
        let payload = &datagram[start..received];
        let length = payload.len().min(buf.len());
        buf[..length].copy_from_slice(&payload[..length]);

        Ok((length, source))
    }

    /// The local address of the tunnel's UDP socket.
    ///
    /// # Errors
    /// Returns an error if the socket address cannot be retrieved.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.socket.local_addr()?)
    }

    /// Sets the read timeout of the tunnel's UDP socket.
    ///
    /// # Errors
    /// Returns an error if the timeout cannot be applied.
    pub fn set_read_timeout(&self, timeout: Option<std::time::Duration>) -> Result<()> {
        Ok(self.socket.set_read_timeout(timeout)?)
    }
}
