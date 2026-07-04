//! L4 — ordinary UDP via `rustix`. The kernel builds IP + Ethernet + checksums; this is
//! the honest top rung of the ladder (no mangling below L4), and it needs no privilege.

use std::io;
use std::net::SocketAddrV4;
use std::os::fd::OwnedFd;

use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketFlags, SocketType, socket_with};

use crate::{Layer, OpenError, Protocol, ProtocolExt, RawIo};

/// A connected UDP socket.
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
