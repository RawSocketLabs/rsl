//! **`network_send`** — hand a complete IPv4 datagram to the kernel at L3 via [`NetworkSocket`].
//!
//! The `network` rung takes the whole IP header from the caller (`IPPROTO_RAW`/`IP_HDRINCL`);
//! the kernel adds only the link layer and routes by the datagram's destination — so no
//! interface is named here (see `list_interfaces`/`link_send` for the L2 story). Per raw(7)
//! the kernel still fills the total length, the IP checksum, and a zero source address or
//! packet id; everything else is yours, verbatim. Needs `CAP_NET_RAW`.
//!
//! Run with: `sudo -E cargo run -p rawsock --example network_send --features network -- [dst]`
//! (default `127.0.0.1`).
#![allow(clippy::print_stdout)]

use rawsock::{NetworkSocket, RawIo, capabilities};
use std::net::Ipv4Addr;

fn main() {
    let dst: Ipv4Addr = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(Ipv4Addr::LOCALHOST);

    // A minimal IPv4 + UDP datagram. Source is left 0.0.0.0 and the lengths/IP checksum 0, so
    // the kernel fills them (raw(7)); the UDP checksum 0 means "unchecked", legal for IPv4.
    let datagram = ipv4_udp(dst, 40000, 9, b"rawsock network_send");

    println!("built {}-byte IPv4/UDP datagram for {dst}", datagram.len());
    if !capabilities().network {
        println!("(no CAP_NET_RAW — built only; rerun privileged to send)");
        return;
    }

    let mut sock = NetworkSocket::open().expect("open raw L3 socket");
    sock.connect(dst).expect("connect");
    match sock.send_raw(&datagram) {
        Ok(n) => println!("sent {n} bytes via a raw L3 socket \u{2713}"),
        Err(e) => println!("send failed: {e}"),
    }
}

/// A 20-byte IPv4 header (source and derived fields left 0 for the kernel) wrapping an 8-byte
/// UDP header with checksum disabled, plus `payload`.
fn ipv4_udp(dst: Ipv4Addr, sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let udp_len = u16::try_from(8 + payload.len()).expect("payload fits a UDP datagram");
    let mut d = vec![
        0x45, 0x00, 0x00, 0x00, // ver/IHL, DSCP/ECN, total length (kernel fills)
        0x00, 0x00, 0x00, 0x00, // id (kernel fills), flags/fragment
        0x40, 0x11, 0x00, 0x00, // TTL 64, protocol 17 (UDP), header checksum (kernel fills)
    ];
    d.extend_from_slice(&Ipv4Addr::UNSPECIFIED.octets()); // src 0.0.0.0 (kernel fills)
    d.extend_from_slice(&dst.octets());
    d.extend_from_slice(&sport.to_be_bytes());
    d.extend_from_slice(&dport.to_be_bytes());
    d.extend_from_slice(&udp_len.to_be_bytes());
    d.extend_from_slice(&[0x00, 0x00]); // UDP checksum 0 = unchecked (valid for IPv4)
    d.extend_from_slice(payload);
    d
}
