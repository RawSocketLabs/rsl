use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};

use binrw::{io::Cursor, BinRead, BinWrite};

use crate::client::client::{reply_socket_addr, Client, TargetAddr};
use crate::error::{Result, SocksError};
use crate::v5::{Address, Command, UdpHeader, UdpHeaderBuilder};

/// Maximum UDP request header size: reserved + frag + address type +
/// length-prefixed domain + port.
const MAX_UDP_HEADER: usize = 2 + 1 + 1 + 256 + 2;

/// The largest UDP datagram (SOCKS request header + payload) the tunnel emits.
pub const MAX_DATAGRAM: usize = 65_535;

/// Size of the SOCKS UDP request header prefixed to a datagram bound for
/// `target`: RSV(2) + FRAG(1) + ATYP(1) + DST.ADDR + DST.PORT(2).
fn header_len(target: &TargetAddr) -> usize {
    let addr = match target {
        TargetAddr::Ip(addr) if addr.is_ipv4() => 4,
        TargetAddr::Ip(_) => 16,
        TargetAddr::Domain(domain, _) => 1 + domain.len(),
    };
    2 + 1 + 1 + addr + 2
}

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

    /// The maximum payload, in bytes, that can be sent to `target` in a single
    /// datagram. This is strictly smaller than the raw datagram capacity the
    /// OS provides, because the SOCKS UDP request header consumes part of each
    /// datagram — callers must size payloads against this, not the socket's
    /// buffer.
    //~ implements rfc1928#7/must.2ac621
    pub fn max_payload(&self, target: &TargetAddr) -> usize {
        MAX_DATAGRAM.saturating_sub(header_len(target))
    }

    /// Sends `payload` to `target` through the relay.
    ///
    /// # Errors
    /// Returns [`SocksError::Validation`] when the payload exceeds
    /// [`max_payload`](Self::max_payload) for the target, or an error if the
    /// header cannot be constructed or the datagram cannot be sent.
    pub fn send_to(&self, target: impl Into<TargetAddr>, payload: &[u8]) -> Result<()> {
        let target = target.into();
        let max = self.max_payload(&target);
        if payload.len() > max {
            return Err(SocksError::Validation(format!(
                "payload of {} bytes exceeds the {} available after the SOCKS UDP header",
                payload.len(),
                max
            )));
        }
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

    /// The relay's UDP address (BND.ADDR/BND.PORT from the UDP ASSOCIATE
    /// reply) that this tunnel sends datagrams to.
    pub fn relay_addr(&self) -> SocketAddr {
        self.relay
    }

    /// Sets the read timeout of the tunnel's UDP socket.
    ///
    /// # Errors
    /// Returns an error if the timeout cannot be applied.
    pub fn set_read_timeout(&self, timeout: Option<std::time::Duration>) -> Result<()> {
        Ok(self.socket.set_read_timeout(timeout)?)
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn header_len_matches_address_type() {
        let v4 = TargetAddr::Ip(SocketAddr::from(([1, 2, 3, 4], 53)));
        let v6 = TargetAddr::Ip(SocketAddr::from(([0u16; 8], 53)));
        let domain = TargetAddr::Domain("example.com".into(), 53); // 11 chars

        assert_eq!(header_len(&v4), 10); // 2+1+1+4+2
        assert_eq!(header_len(&v6), 22); // 2+1+1+16+2
        assert_eq!(header_len(&domain), 7 + 11); // 2+1+1+(1+11)+2

        // The reported payload capacity is always below the raw datagram size.
        assert!(MAX_DATAGRAM - header_len(&v4) < MAX_DATAGRAM);
        assert_eq!(MAX_DATAGRAM - header_len(&domain), 65_535 - 18);
    }
}
