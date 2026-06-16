//! Generic recursion over `Source` (ROADMAP Phase 1, chunk B1): one derived
//! codec decodes over the in-memory `BitReader` (slice) **and** the forward
//! `StreamBitReader` (any `std::io::Read`) — the "one codec, any source" payoff.

use bnb::{BitDecode, BitEncode, BitReader, BitWriter, StreamBitReader, u4, u12};

#[derive(BitDecode, BitEncode, Debug, PartialEq, Eq)]
struct Word {
    a: u4,
    b: u12, // 16 bits; all <= 64 so the streaming reader handles it too
}

#[test]
fn decodes_over_slice_and_stream_identically() {
    let word = Word {
        a: u4::new(0xA),
        b: u12::new(0xBCD),
    };
    let mut w = BitWriter::new();
    word.bit_encode(&mut w).unwrap();
    let bytes = w.into_bytes();
    assert_eq!(bytes, [0xAB, 0xCD]);

    // Source 1 — in-memory slice cursor (random-access, full power).
    let mut slice = BitReader::new(&bytes);
    assert_eq!(Word::bit_decode(&mut slice).unwrap(), word);

    // Source 2 — a forward `Read` (`&[u8]` is `Read` but NOT `Seek`); same code,
    // no rewrite, no Seek requirement.
    let mut stream = StreamBitReader::new(&bytes[..]);
    assert_eq!(Word::bit_decode(&mut stream).unwrap(), word);
}
