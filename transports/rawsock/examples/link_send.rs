//! **`link_send`** — put a complete Ethernet frame on a named interface via [`LinkSocket`].
//!
//! The `link` rung takes the whole frame from the caller (`AF_PACKET`); the NIC appends only
//! the FCS. The socket is bound to one interface by name — pass it, or let the example pick
//! the first non-loopback interface that is up (see `list_interfaces`). Needs `CAP_NET_RAW`.
//!
//! Run with: `sudo -E cargo run -p rawsock --example link_send --features link -- [iface]`.
#![allow(clippy::print_stdout)]

use rawsock::{LinkSocket, RawIo, capabilities};

fn main() {
    let Some(iface) = std::env::args().nth(1).or_else(default_interface) else {
        println!("no interface given and no non-loopback interface is up");
        return;
    };

    // A minimum-size (60-byte) frame: broadcast dst, a locally-administered src MAC, an
    // experimental EtherType, zero-padded. Sent verbatim — the sink never validates it.
    let mut frame = Vec::with_capacity(60);
    frame.extend_from_slice(&[0xff; 6]); // dst: broadcast
    frame.extend_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x01]); // src: locally administered
    frame.extend_from_slice(&0x88b5u16.to_be_bytes()); // experimental EtherType
    frame.extend_from_slice(b"rawsock link_send");
    frame.resize(60, 0);

    println!("built {}-byte frame for {iface}", frame.len());
    if !capabilities().link {
        println!("(no CAP_NET_RAW — built only; rerun privileged to send)");
        return;
    }

    let mut sock = match LinkSocket::open(&iface) {
        Ok(sock) => sock,
        Err(e) => {
            println!("open {iface} failed: {e}");
            return;
        }
    };
    match sock.send_raw(&frame) {
        Ok(n) => println!("sent {n} bytes on {iface} via a raw L2 socket \u{2713}"),
        Err(e) => println!("send on {iface} failed: {e}"),
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
