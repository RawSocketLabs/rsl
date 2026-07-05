//! **spoof_udp** — compose a UDP datagram with a **forged source IP** and inject it at L3.
//!
//! The dual-use payoff: the `ip` layer stores whatever source address you give it, so we can
//! impersonate another host. Run with `cargo run -p demos --example spoof_udp` (composing works
//! unprivileged; the raw send needs `CAP_NET_RAW`). **Authorized testing only.**
#![allow(clippy::print_stdout)]

use ip::{Ip, Ipv4Header};
use rawsock::{NetworkSocket, ProtocolExt, RawIo, capabilities};
use std::net::Ipv4Addr;
use udp::Udp;

fn main() {
    let forged_src = Ipv4Addr::new(10, 0, 0, 99); // impersonate this host
    let target = Ipv4Addr::new(10, 0, 0, 2);

    // Ip(Udp(payload)) with a forged source address; the IP layer fills length + both checksums.
    let packet = Ip::new(
        Ipv4Header::datagram(forged_src, target, 17, 0), // 17 = UDP
        Udp::new(40000, 53, b"spoofed".to_vec()),
    );
    let bytes = packet.encode();

    println!(
        "forged IPv4+UDP  {forged_src} -> {target}  ({} bytes)",
        bytes.len()
    );
    println!("  {:02x?}", bytes);

    if capabilities().network {
        let mut sock = NetworkSocket::open().expect("open raw L3 socket");
        sock.connect(target).expect("connect");
        match sock.send_raw(&bytes) {
            Ok(n) => println!("sent {n} bytes via a raw L3 socket \u{2713}"),
            Err(e) => println!("send failed: {e}"),
        }
    } else {
        println!("(no CAP_NET_RAW \u{2014} composed only; run with the capability to inject)");
    }
}
