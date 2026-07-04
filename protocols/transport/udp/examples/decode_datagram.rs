//! **decode_datagram** — decode a UDP header and build one back.
//!
//! Run with: `cargo run -p udp --example decode_datagram`
#![allow(clippy::print_stdout)]

use udp::UdpHeader;

fn main() {
    // A DNS query datagram header: src 40000 -> dst 53, 29-byte payload.
    let wire = [0x9c, 0x40, 0x00, 0x35, 0x00, 0x25, 0xab, 0xcd];
    let h = UdpHeader::decode_exact(&wire).unwrap();
    println!(
        "{} -> {}  length={} (payload {}), checksum={:#06x}",
        h.src_port,
        h.dst_port,
        h.length,
        h.payload_len(),
        h.checksum
    );

    let built = UdpHeader::for_payload(40000, 53, 29);
    println!("built header bytes: {:02x?}", built.to_bytes().unwrap());
}
