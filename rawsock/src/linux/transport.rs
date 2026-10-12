//! L4 — ordinary UDP via `rustix`. The kernel builds IP + Ethernet + checksums; this is
//! the honest top rung of the ladder (no mangling below L4), and it needs no privilege.

use std::io;
use std::net::SocketAddrV4;
use std::os::fd::OwnedFd;

use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketFlags, SocketType, socket_with};

use crate::{Layer, OpenError, Protocol, ProtocolExt, RawIo};

/// A connected UDP socket.
///
/// ```
/// use rawsock::{RawIo, TransportSocket};
/// use std::net::{SocketAddr, UdpSocket};
/// use std::time::Duration;
///
/// let peer = UdpSocket::bind("127.0.0.1:0")?;
/// peer.set_read_timeout(Some(Duration::from_secs(2)))?;
/// let SocketAddr::V4(addr) = peer.local_addr()? else { unreachable!() };
///
/// let mut sock = TransportSocket::udp()?;
/// sock.connect(addr)?;
/// sock.send_raw(b"payload")?; // the kernel adds UDP, IP, and link headers
///
/// let mut buf = [0u8; 16];
/// let n = peer.recv(&mut buf)?;
/// assert_eq!(&buf[..n], b"payload");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct TransportSocket {
    fd: OwnedFd,
}

impl TransportSocket {
    /// Open an IPv4 UDP socket (unprivileged).
    ///
    /// # Errors
    /// [`OpenError`] if the socket cannot be created.
    pub fn udp() -> Result<Self, OpenError> {
        let fd = socket_with(
            AddressFamily::INET,
            SocketType::DGRAM,
            SocketFlags::empty(),
            None,
        )
        .map_err(io::Error::from)?;
        Ok(Self { fd })
    }

    /// Connect to a peer so [`RawIo::send_raw`]/[`send`](Self::send) target it.
    ///
    /// # Errors
    /// Any connect failure ([`io::Error`]).
    pub fn connect(&self, peer: SocketAddrV4) -> io::Result<()> {
        rustix::net::connect(&self.fd, &peer).map_err(Into::into)
    }

    /// Encode `p` (compliant) and transmit it.
    ///
    /// # Errors
    /// Any transmit failure ([`io::Error`]).
    pub fn send(&mut self, p: &impl Protocol) -> io::Result<usize> {
        self.send_raw(&p.encode())
    }
}

impl RawIo for TransportSocket {
    fn send_raw(&mut self, bytes: &[u8]) -> io::Result<usize> {
        rustix::net::send(&self.fd, bytes, SendFlags::empty()).map_err(Into::into)
    }
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        rustix::net::recv(&self.fd, buf, RecvFlags::empty())
            .map(|(n, _)| n)
            .map_err(Into::into)
    }
    fn layer(&self) -> Layer {
        Layer::Transport
    }
}
