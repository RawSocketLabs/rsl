//! **`link_sniff`** — read a few raw Ethernet frames from an interface via [`LinkSocket`].
//!
//! The counterpart to `link_send`: an `AF_PACKET` socket bound to one interface sees every
//! frame on it (`ETH_P_ALL`). This reads [`RawIo::recv`] at L2 and prints each frame's size
//! and 14-byte Ethernet header, then exits after a fixed count. `recv` blocks, so on an idle
//! link it waits for traffic. Pass the interface, or let it pick the first non-loopback
//! interface that is up (see `list_interfaces`). Needs `CAP_NET_RAW`.
//!
//! Run with: `sudo -E cargo run -p rawsock --example link_sniff --features link -- [iface] [count]`
//! (default count 5).
#![allow(clippy::print_stdout)]

use rawsock::{LinkSocket, RawIo, capabilities};

fn main() {
    if !capabilities().link {
        println!("no CAP_NET_RAW — cannot open an L2 socket (rerun privileged)");
        return;
    }
    let Some(iface) = std::env::args().nth(1).or_else(default_interface) else {
        println!("no interface given and no non-loopback interface is up");
        return;
    };
    let count: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let mut sock = match LinkSocket::open(&iface) {
        Ok(sock) => sock,
        Err(e) => {
            println!("open {iface} failed: {e}");
            return;
        }
    };
    println!("reading {count} frame(s) on {iface} (blocks until traffic arrives)...");

    let mut buf = vec![0u8; 65_536];
    for i in 0..count {
        match sock.recv(&mut buf) {
            Ok(n) => {
                let header = &buf[..n.min(14)];
                println!("  [{i}] {n} bytes, ethernet header {header:02x?}");
            }
            Err(e) => {
                println!("  recv failed: {e}");
                return;
            }
        }
    }
}

/// The first non-loopback interface whose `operstate` is `up`, read from `/sys/class/net`.
fn default_interface() -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir("/sys/class/net")
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|name| name != "lo")
        .collect();
    names.sort();
    names.into_iter().find(|name| {
        std::fs::read_to_string(format!("/sys/class/net/{name}/operstate"))
            .is_ok_and(|s| s.trim() == "up")
    })
}
