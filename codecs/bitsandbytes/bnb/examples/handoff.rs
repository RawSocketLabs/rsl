//! **`handoff`** — a typed handshake, then the connection belongs to another protocol.
//!
//! An upgrade (a proxy's CONNECT reply, STARTTLS, a version exchange before a raw
//! tunnel) reads a few messages and then gives the connection to code that knows nothing
//! about them. The trap: a transport hands over whatever has arrived, so the read that
//! completed the handshake usually brought the first bytes of what follows, and they now
//! sit in the [`MessageStream`]'s buffer. Taking the socket back with `get_mut()` loses
//! them. Three ways not to: read raw bytes *through* the wrapper, move the stream and its
//! buffer together with `into_parts` / `from_parts`, or use `try_into_inner`, which gives
//! the stream back only when nothing is held.
//!
//! Run with: `cargo run -p bitsandbytes --example handoff --features mock`
//!
//! [`MessageStream`]: bnb::MessageStream

use bnb::{MessageStream, MockStream, bin};
use std::io::Read;

#[bin(big, magic = 0x5550u16)] // "UP"
#[derive(Debug, PartialEq)]
struct Upgrade {
    version: u8,
    #[brw(count_prefix = u8)]
    token: Vec<u8>,
}

/// The next protocol: it reads bytes and has never heard of `Upgrade`.
fn tunnel(mut bytes: impl Read) -> Vec<u8> {
    let mut all = Vec::new();
    bytes.read_to_end(&mut all).unwrap();
    all
}

/// A connection whose first read delivers the handshake and the tunnel's first bytes.
fn connection() -> MessageStream<MockStream> {
    let mut stream = MockStream::new();
    stream.push_inbound(b"UP\x01\x02\xAA\xBBtunnel");
    MessageStream::new(stream)
}

#[allow(clippy::print_stdout)] // This CLI demo intentionally prints its observable results.
fn main() {
    let hello = Upgrade {
        version: 1,
        token: vec![0xAA, 0xBB],
    };

    // The wrapper is itself a reader: it gives what it holds first, then the stream.
    let mut conn = connection();
    assert_eq!(conn.read_message::<Upgrade>().unwrap(), hello);
    assert_eq!(tunnel(&mut conn), b"tunnel");
    println!("through the wrapper : the tunnel reads `tunnel`");

    // Hand the stream and what was read ahead to the next owner, together.
    let mut conn = connection();
    assert_eq!(conn.read_message::<Upgrade>().unwrap(), hello);
    let (stream, held) = conn.into_parts();
    assert_eq!(held.bit_len(), b"tunnel".len() * 8);
    let next = MessageStream::from_parts(stream, held);
    assert_eq!(tunnel(next), b"tunnel");
    println!("into_parts          : six bytes read ahead travel with the stream");

    // Ask for the bare stream: refused while bytes are held, and nothing is lost.
    let mut conn = connection();
    assert_eq!(conn.read_message::<Upgrade>().unwrap(), hello);
    let Err(mut conn) = conn.try_into_inner() else {
        unreachable!("six bytes are still held")
    };
    let mut ahead = [0; 6];
    conn.read_exact(&mut ahead).unwrap();
    assert_eq!(&ahead, b"tunnel");
    let Ok(bare) = conn.try_into_inner() else {
        unreachable!("nothing is held now")
    };
    assert_eq!(tunnel(bare), b"");
    println!("try_into_inner      : refused until the held bytes were read");

    println!("all checks passed");
}
