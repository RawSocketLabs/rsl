//! **frame_stack** — compose the whole stack, `Ethernet(Ip(Icmp(echo request)))`, so every
//! layer fills its derived fields (EtherType, IP length/protocol/checksum, ICMP checksum), then
//! hand the frame to a rawsock sink (Loopback — no privilege).
//!
//! Run with: `cargo run -p ethernet --features inject --example frame_stack`
#![allow(clippy::print_stdout)]

use ethernet::{BROADCAST, Ethernet, EthernetHeader};
use ethertype::EtherType;
use icmp::{Icmp, IcmpHeader};
use ip::{Ip, Ipv4Header};
use rawsock::{Layer, Loopback, Protocol, ProtocolExt, RawIo, internet_checksum};
use std::net::Ipv4Addr;

fn main() {
    let (src_ip, dst_ip) = (Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2));
    let src_mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];

    // Ethernet( Ip( Icmp( echo request ) ) ) — the full L2→L4 stack.
    let frame = Ethernet::new(
        EthernetHeader {
            dst: BROADCAST,
            src: src_mac,
            ethertype: EtherType::Custom(0), // filled from the payload on encode
        },
        Ip::new(
            Ipv4Header::datagram(src_ip, dst_ip, 1, 0),
            Icmp::new(IcmpHeader::echo_request(0x1234, 1), b"pingpong".to_vec()),
        ),
    );
    let bytes = frame.encode();

    println!("full Ethernet+IPv4+ICMP frame, {} bytes:", bytes.len());
    println!("  {:02x?}", bytes);
    println!(
        "  ethertype={:#06x} (from the IP payload), layer={}",
        u16::from_be_bytes([bytes[12], bytes[13]]),
        frame.layer(),
    );
    // Both nested checksums still verify over their regions (offset by the 14-byte L2 header).
    assert_eq!(internet_checksum(&bytes[14..34]), 0); // IPv4 header
    assert_eq!(internet_checksum(&bytes[34..]), 0); // ICMP message
    println!("  IPv4 + ICMP checksums verify through the L2 frame ✓");

    let mut sink = Loopback::new(Layer::Link);
    let n = sink.send_raw(&bytes).unwrap();
    println!("sent {n} bytes through the rawsock L2 sink ✓ (the NIC would append the FCS)");
}
