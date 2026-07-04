//! **who_has** — compose an `Ethernet(ArpPacket)` frame: an ARP request broadcast asking who
//! owns an IP, then hand it to a rawsock sink (Loopback — no privilege). The Ethernet layer
//! auto-fills its EtherType to `0x0806` from the ARP payload.
//!
//! Run with: `cargo run -p arp --features inject --example who_has`
#![allow(clippy::print_stdout)]

use arp::ArpPacket;
use ethernet::{BROADCAST, Ethernet, EthernetHeader};
use ethertype::EtherType;
use rawsock::{Layer, Loopback, ProtocolExt, RawIo};
use std::net::Ipv4Addr;

fn main() {
    let src_mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
    let (sender_ip, target_ip) = (Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2));

    // "who has 10.0.0.2? tell 10.0.0.1", broadcast at L2.
    let frame = Ethernet::new(
        EthernetHeader {
            dst: BROADCAST,
            src: src_mac,
            ethertype: EtherType::Custom(0), // filled from the ARP payload on encode
        },
        ArpPacket::request(src_mac, sender_ip, target_ip),
    );
    let bytes = frame.encode();

    println!("Ethernet + ARP request, {} bytes:", bytes.len());
    println!("  {:02x?}", bytes);
    println!(
        "  ethertype={:#06x} (ARP), oper={}",
        u16::from_be_bytes([bytes[12], bytes[13]]),
        u16::from_be_bytes([bytes[14 + 6], bytes[14 + 7]]), // oper is at ARP offset 6
    );

    let mut sink = Loopback::new(Layer::Link);
    let n = sink.send_raw(&bytes).unwrap();
    println!("sent {n} bytes through the rawsock L2 sink ✓");
}
