//! L3 — raw IPv4 via `rustix` (`IPPROTO_RAW` ⇒ `IP_HDRINCL`). The caller supplies the whole
//! IP datagram (header + payload); the kernel builds only the link layer. Needs `CAP_NET_RAW`.

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::os::fd::OwnedFd;

use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketFlags, SocketType, ipproto, socket_with,
};

use crate::{Layer, OpenError, Protocol, ProtocolExt, RawIo};

/// A raw IPv4 socket (`IPPROTO_RAW`): you provide the entire IP datagram; the kernel adds
/// only L2. Requires `CAP_NET_RAW` — the dual-use rung where forged IP headers reach the wire.
pub struct NetworkSocket {
    fd: OwnedFd,
}

impl NetworkSocket {
    /// Open a raw IPv4 socket.
    ///
    /// # Errors
    /// [`OpenError::PermissionDenied`] without `CAP_NET_RAW`; otherwise [`OpenError::Io`].
    pub fn open() -> Result<Self, OpenError> {
        let fd = socket_with(
            AddressFamily::INET,
            SocketType::RAW,
            SocketFlags::empty(),
            Some(ipproto::RAW),
        )
        .map_err(io::Error::from)?;
        Ok(Self { fd })
    }

    /// Set the default destination for [`send`](Self::send). A raw IP socket ignores the port;
    /// the kernel routes by the datagram's own destination address (which should match `dst`).
    ///
    /// # Errors
    /// Any connect failure ([`io::Error`]).
    pub fn connect(&self, dst: Ipv4Addr) -> io::Result<()> {
        rustix::net::connect(&self.fd, &SocketAddrV4::new(dst, 0)).map_err(Into::into)
    }

    /// Encode `p` (compliant) and transmit the datagram.
    ///
    /// # Errors
    /// Any transmit failure ([`io::Error`]).
    pub fn send(&mut self, p: &impl Protocol) -> io::Result<usize> {
        self.send_raw(&p.encode())
    }
}

impl RawIo for NetworkSocket {
    fn send_raw(&mut self, bytes: &[u8]) -> io::Result<usize> {
        rustix::net::send(&self.fd, bytes, SendFlags::empty()).map_err(Into::into)
    }
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        rustix::net::recv(&self.fd, buf, RecvFlags::empty())
            .map(|(n, _)| n)
            .map_err(Into::into)
    }
    fn layer(&self) -> Layer {
        Layer::Network
    }
}
