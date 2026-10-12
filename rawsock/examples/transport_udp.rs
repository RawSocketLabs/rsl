//! **`transport_udp`** — the honest top rung: an ordinary UDP datagram via [`TransportSocket`].
//!
//! The kernel builds IP + Ethernet + checksums and routes by destination; no interface is
//! named and no privilege is needed. Compare with `network_send` (L3) and `link_send` (L2),
//! which hand the kernel progressively more of the stack verbatim.
//!
//! Run with: `cargo run -p rawsock --example transport_udp -- [host:port]`
//! (default `127.0.0.1:9`, the discard port).
#![allow(clippy::print_stdout)]

use rawsock::{RawIo, TransportSocket};
use std::net::{SocketAddr, SocketAddrV4, ToSocketAddrs};

fn main() {
    let target = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9".to_owned());
    let Some(addr) = resolve_v4(&target) else {
        println!("could not resolve {target} to an IPv4 address");
        return;
    };

    let mut sock = TransportSocket::udp().expect("open UDP socket");
    sock.connect(addr).expect("connect");
    match sock.send_raw(b"rawsock transport_udp") {
        Ok(n) => println!("sent {n} bytes to {addr} (kernel added UDP/IP/link) \u{2713}"),
        Err(e) => println!("send to {addr} failed: {e}"),
    }
}

/// Resolve `host:port` and return the first IPv4 address, or `None` if there is none.
fn resolve_v4(target: &str) -> Option<SocketAddrV4> {
    target.to_socket_addrs().ok()?.find_map(|a| match a {
        SocketAddr::V4(v4) => Some(v4),
        SocketAddr::V6(_) => None,
    })
}
