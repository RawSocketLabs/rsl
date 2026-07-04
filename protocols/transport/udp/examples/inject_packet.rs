//! **inject_packet** — compose a UDP packet over rawsock, checksum it against an IPv4
//! pseudo-header, and hand the bytes to a rawsock sink (Loopback — no privilege needed).
//!
//! Run with: `cargo run -p udp --features inject --example inject_packet`
#![allow(clippy::print_stdout)]

use rawsock::{Context, Layer, Loopback, Protocol, Pseudo, RawIo};
use std::net::Ipv4Addr;
use udp::Udp;

fn main() {
    // A UDP datagram 40000 -> 53 carrying a 4-byte payload, sent "from" 10.0.0.1 to 10.0.0.2.
    let packet = Udp::new(40000, 53, vec![0xC0, 0xFF, 0xEE, 0x00]);
    let pseudo = Pseudo {
        src: Ipv4Addr::new(10, 0, 0, 1),
        dst: Ipv4Addr::new(10, 0, 0, 2),
        protocol: 17,
    };

    // Compliant encode: length + checksum computed from the pseudo-header.
    let bytes = packet.encode_with(&Context {
        pseudo: Some(pseudo),
    });
    println!("composed {} bytes: {:02x?}", bytes.len(), bytes);
    println!(
        "  length={}  checksum={:#06x}",
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
    );

    // Hand it to a rawsock sink. Loopback records instead of transmitting — swap in a
    // privileged L3 socket (when the network backend lands) to actually put it on the wire.
    let mut sink = Loopback::new(Layer::Network);
    let n = sink.send_raw(&bytes).unwrap();
    println!("sent {n} bytes through the rawsock sink");
    assert_eq!(sink.last_sent(), Some(bytes.as_slice()));
    println!("sink recorded the packet ✓");
}
