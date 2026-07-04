//! **ping** — compose a full `Ip(Icmp(echo request))` datagram (a ping), so the ICMP checksum
//! covers the message and the IP layer wraps + checksums it, then hand the bytes to a rawsock
//! sink (Loopback — no privilege).
//!
//! Run with: `cargo run -p icmp --features inject --example ping`
#![allow(clippy::print_stdout)]

use icmp::{Icmp, IcmpHeader};
use ip::{Ip, Ipv4Header};
use rawsock::{Layer, Loopback, ProtocolExt, RawIo, internet_checksum};
use std::net::Ipv4Addr;

fn main() {
    let (src, dst) = (Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2));

    // An Echo Request (id 0x1234, seq 1) carrying 8 bytes of ping data, wrapped in IPv4.
    let packet = Ip::new(
        Ipv4Header::datagram(src, dst, 1, 0), // protocol 1 = ICMP; length/checksum filled on encode
        Icmp::new(IcmpHeader::echo_request(0x1234, 1), b"pingpong".to_vec()),
    );
    let bytes = packet.encode();

    println!("IPv4 + ICMP echo request, {} bytes:", bytes.len());
    println!("  {:02x?}", bytes);
    let icmp = &bytes[20..];
    println!(
        "  icmp type={} code={} checksum={:#06x}",
        icmp[0],
        icmp[1],
        u16::from_be_bytes([icmp[2], icmp[3]]),
    );
    // Both checksums verify to 0 over their regions (RFC 1071).
    assert_eq!(internet_checksum(&bytes[..20]), 0); // IPv4 header
    assert_eq!(internet_checksum(icmp), 0); // ICMP message (self-contained)
    println!("  IPv4 header + ICMP checksums verify ✓");

    let mut sink = Loopback::new(Layer::Network);
    let n = sink.send_raw(&bytes).unwrap();
    println!("sent {n} bytes through the rawsock sink ✓");
}
