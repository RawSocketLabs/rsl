//! **inject_segment** — compose a TCP SYN over rawsock, checksum it against an IPv4
//! pseudo-header, and hand the bytes to a rawsock sink (Loopback — no privilege needed).
//!
//! Run with: `cargo run -p tcp --features inject --example inject_segment`
#![allow(clippy::print_stdout)]

use rawsock::{Context, Layer, Loopback, Protocol, Pseudo, RawIo};
use std::net::Ipv4Addr;
use tcp::{Control, Tcp, TcpHeader};

fn main() {
    // A bare SYN from 10.0.0.1:40000 to 10.0.0.2:80, no payload.
    let syn = TcpHeader::segment(
        40000,
        80,
        0x1000_0000,
        0,
        Control::new().with_syn(true),
        65535,
        vec![],
    );
    let segment = Tcp::new(syn, Vec::new());
    let pseudo = Pseudo {
        src: Ipv4Addr::new(10, 0, 0, 1),
        dst: Ipv4Addr::new(10, 0, 0, 2),
        protocol: 6,
    };

    let bytes = segment.encode_with(&Context {
        pseudo: Some(pseudo),
    });
    println!("composed {} bytes: {:02x?}", bytes.len(), bytes);
    println!(
        "  checksum={:#06x}",
        u16::from_be_bytes([bytes[16], bytes[17]])
    );

    let mut sink = Loopback::new(Layer::Network);
    let n = sink.send_raw(&bytes).unwrap();
    println!("sent {n} bytes through the rawsock sink");
    assert_eq!(sink.last_sent(), Some(bytes.as_slice()));
    println!("sink recorded the segment ✓");
}
