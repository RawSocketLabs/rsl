//! **inject_stack** — compose a full `Ip(Udp(payload))` datagram, so the IP layer supplies the
//! UDP checksum's pseudo-header and computes its own header checksum, then hand the bytes to a
//! rawsock sink (Loopback — no privilege).
//!
//! Run with: `cargo run -p ip --features inject --example inject_stack`
#![allow(clippy::print_stdout)]

use ip::{Ip, Ipv4Header};
use rawsock::{Layer, Loopback, ProtocolExt, RawIo, internet_checksum};
use std::net::Ipv4Addr;
use udp::Udp;

fn main() {
    let (src, dst) = (Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2));
    let payload = vec![0xC0, 0xFF, 0xEE, 0x00];

    // Ip( Udp( payload ) ): the IP layer fills protocol + total_length + both checksums.
    let packet = Ip::new(
        Ipv4Header::datagram(src, dst, 17, 0), // total_length/checksum filled on encode
        Udp::new(40000, 53, payload),
    );
    let bytes = packet.encode();

    println!("full IPv4+UDP datagram, {} bytes:", bytes.len());
    println!("  {:02x?}", bytes);
    println!(
        "  ip.total_length={}  ip.protocol={}  ip.checksum={:#06x}",
        u16::from_be_bytes([bytes[2], bytes[3]]),
        bytes[9],
        u16::from_be_bytes([bytes[10], bytes[11]]),
    );
    // The IPv4 header checksum verifies to 0 over the 20-byte header.
    assert_eq!(internet_checksum(&bytes[..20]), 0);
    println!("  IPv4 header checksum verifies ✓");

    let mut sink = Loopback::new(Layer::Network);
    let n = sink.send_raw(&bytes).unwrap();
    println!("sent {n} bytes through the rawsock sink ✓");
}
