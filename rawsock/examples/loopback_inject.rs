//! **loopback_inject** — the dual-use sink in miniature: `RawIo::send_raw` transmits
//! exactly the bytes you give it, no validation. Uses the in-memory [`Loopback`] backend
//! so it runs anywhere, unprivileged.
//!
//! Run with: `cargo run --example loopback_inject`

use rawsock::compose::internet_checksum;
use rawsock::{Layer, Loopback, RawIo};

fn main() {
    let mut sink = Loopback::new(Layer::Link);

    // A well-formed-ish "frame": dst MAC, src MAC, ethertype, payload.
    let mut frame = Vec::new();
    frame.extend_from_slice(&[0xff; 6]); // broadcast dst
    frame.extend_from_slice(&[0x02, 0, 0, 0, 0, 1]); // src
    frame.extend_from_slice(&0x88b5u16.to_be_bytes()); // experimental ethertype
    frame.extend_from_slice(b"hello");
    sink.send_raw(&frame).unwrap();

    // The dual-use point: a deliberately malformed, 1-byte "frame" goes out verbatim —
    // the sink never validates, synthesizes, or fixes anything.
    sink.send_raw(&[0x00]).unwrap();

    println!("transmitted {} frames (verbatim):", sink.sent().len());
    for (i, f) in sink.sent().iter().enumerate() {
        println!("  [{i}] {} bytes: {:02x?}", f.len(), f);
    }

    // The compliant half lives in the compose model; here's its checksum primitive.
    let header = [0x45, 0x00, 0x00, 0x1c, 0xde, 0xad, 0x00, 0x00, 0x40, 0x11];
    println!(
        "\ninternet_checksum(sample IP header) = {:#06x}",
        internet_checksum(&header)
    );
}
