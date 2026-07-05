//! L2 — raw Ethernet via `AF_PACKET` (rustix). The caller supplies the whole Ethernet frame
//! (dst/src MAC, EtherType, payload); the NIC computes and appends the FCS. Needs `CAP_NET_RAW`.
//!
//! rustix 1.1.4 (the latest) has no safe link-layer socket address type, so binding the socket
//! to an interface needs **one** `unsafe` call to build a `sockaddr_ll` — the crate's only
//! `unsafe`, confined to [`link_addr`]. Everything else stays safe (no `libc`).

use std::io;
use std::os::fd::OwnedFd;

use rustix::net::{
    AddressFamily, RawProtocol, RecvFlags, SendFlags, SocketAddrAny, SocketFlags, SocketType, bind,
    netdevice, socket_with,
};

use crate::{Layer, OpenError, Protocol, ProtocolExt, RawIo};

const AF_PACKET: u16 = 17; // Linux address family for packet sockets.
const ETH_P_ALL: u16 = 0x0003; // every protocol, in host order (network order applied below).

/// A raw `AF_PACKET` socket bound to an interface: you provide the entire Ethernet frame; the
/// NIC adds only the FCS. Requires `CAP_NET_RAW` — the dual-use rung for forged L2 frames
/// (raw Ethernet, ARP spoofing).
pub struct LinkSocket {
    fd: OwnedFd,
}

impl LinkSocket {
    /// Open a raw `AF_PACKET` socket bound to the interface named `interface` (e.g. `"eth0"`).
    ///
    /// # Errors
    /// [`OpenError::PermissionDenied`] without `CAP_NET_RAW`; otherwise [`OpenError::Io`]
    /// (including an unknown interface name).
    pub fn open(interface: &str) -> Result<Self, OpenError> {
        // htons(ETH_P_ALL) — the packet-socket protocol is in network byte order.
        let proto =
            RawProtocol::new(u32::from(ETH_P_ALL.to_be())).map(rustix::net::Protocol::from_raw);
        let fd = socket_with(
            AddressFamily::PACKET,
            SocketType::RAW,
            SocketFlags::empty(),
            proto,
        )
        .map_err(io::Error::from)?;
        let ifindex = netdevice::name_to_index(&fd, interface).map_err(io::Error::from)?;
        bind(&fd, &link_addr(ifindex)).map_err(io::Error::from)?;
        Ok(Self { fd })
    }

    /// Encode `p` (compliant) and transmit the frame.
    ///
    /// # Errors
    /// Any transmit failure ([`io::Error`]).
    pub fn send(&mut self, p: &impl Protocol) -> io::Result<usize> {
        self.send_raw(&p.encode())
    }
}

/// Build a `sockaddr_ll` bound to `ifindex`, as a rustix [`SocketAddrAny`].
///
/// The crate's **only** `unsafe`: rustix has no safe link-layer address, so we lay out a
/// `sockaddr_ll` and hand its bytes to `SocketAddrAny::read`.
#[allow(unsafe_code)]
fn link_addr(ifindex: u32) -> SocketAddrAny {
    // The Linux `sockaddr_ll` (20 bytes, `#[repr(C)]`). Only `sll_family`/`sll_protocol`/
    // `sll_ifindex` matter for a bind; the hardware-address fields are zero.
    #[repr(C)]
    struct SockaddrLl {
        sll_family: u16,
        sll_protocol: u16,
        sll_ifindex: i32,
        sll_hatype: u16,
        sll_pkttype: u8,
        sll_halen: u8,
        sll_addr: [u8; 8],
    }
    let sll = SockaddrLl {
        sll_family: AF_PACKET,
        sll_protocol: ETH_P_ALL.to_be(),
        sll_ifindex: i32::try_from(ifindex).unwrap_or(0),
        sll_hatype: 0,
        sll_pkttype: 0,
        sll_halen: 0,
        sll_addr: [0; 8],
    };
    // SAFETY: `sll` is a fully-initialized, valid `sockaddr_ll` whose `sll_family` (AF_PACKET)
    // matches its layout; `read` copies exactly `size_of::<SockaddrLl>()` (20) bytes from it,
    // which is >= the sockaddr header and <= `SocketAddrStorage`.
    unsafe {
        SocketAddrAny::read(
            core::ptr::addr_of!(sll).cast(),
            core::mem::size_of::<SockaddrLl>() as u32,
        )
    }
}

impl RawIo for LinkSocket {
    fn send_raw(&mut self, bytes: &[u8]) -> io::Result<usize> {
        rustix::net::send(&self.fd, bytes, SendFlags::empty()).map_err(Into::into)
    }
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        rustix::net::recv(&self.fd, buf, RecvFlags::empty())
            .map(|(n, _)| n)
            .map_err(Into::into)
    }
    fn layer(&self) -> Layer {
        Layer::Link
    }
}
