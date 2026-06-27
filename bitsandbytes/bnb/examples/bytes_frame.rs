//! **bytes_frame** — the `bytes` feature: zero-copy framing with `BytesWriter` / `BytesReader`.
//! Encode a message into a `bytes::Bytes` (the wire frame you'd hand to a socket), then decode
//! straight from an owned `Bytes` with no copy — and cheaply slice/share it (refcounted). This
//! is the foundation the `tokio` `BinCodec` builds on. (A different angle from `framed`, which
//! paired the adapters with `StreamBitReader`.)
//!
//! Run with: `cargo run -p bitsandbytes --example bytes_frame --features bytes`

use bnb::{BitEncode, BytesReader, BytesWriter, bin};

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Packet {
    id: u32,
    #[br(temp)]
    #[bw(calc = self.payload.len() as u16)]
    len: u16,
    #[br(count = len)]
    payload: Vec<u8>,
}

fn main() {
    let pkt = Packet {
        id: 0xABCD,
        payload: b"zero-copy".to_vec(),
    };

    // Encode into a `bytes::Bytes` frame (what you'd write to a socket).
    let mut w = BytesWriter::new();
    pkt.bit_encode(&mut w).unwrap();
    let frame: bytes::Bytes = w.freeze();
    println!("frame: {} bytes  {:02x?}", frame.len(), &frame[..]);

    // Decode straight from the owned `Bytes` — `BytesReader` takes it, no copy.
    let mut r = BytesReader::new(frame.clone());
    let back = Packet::decode(&mut r).unwrap();
    assert_eq!(back, pkt);
    println!("{back:#?}");

    // `Bytes` is refcounted, so a slice is a zero-copy view — e.g. split the 4-byte id off.
    let header = frame.slice(0..4);
    println!("zero-copy header slice: {:02x?}", &header[..]);
    assert_eq!(&header[..], &pkt.id.to_be_bytes());

    println!("all checks passed");
}
