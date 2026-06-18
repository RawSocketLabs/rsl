//! Property-based robustness + round-trip tests across a spread of `#[bin]` message
//! shapes. Three properties, each over random input:
//!
//! 1. **encode ∘ decode = id** — a value encoded then decoded is unchanged.
//! 2. **decode never panics on arbitrary bytes** — the dual-use safety property: a
//!    parser fed hostile/garbage input returns `Ok` or `Err`, never panics.
//! 3. **decode ∘ encode = id** — for fixed-length total parsers, decoding any byte
//!    string of the right length then re-encoding reproduces it (a bijection).

use bnb::{BitEnum, bin, u4, u12};
use proptest::prelude::*;

// --- shapes -------------------------------------------------------------------

/// Byte-aligned header — a total parser, 8 bytes.
#[bin(big)]
#[derive(Debug, Clone, PartialEq)]
struct Header {
    a: u16,
    b: u16,
    c: u32,
}

/// Sub-byte frame — total, 16 bits = 2 bytes.
#[bin(big)]
#[derive(Debug, Clone, PartialEq)]
struct Frame {
    tag: u4,
    len: u12,
}

/// A catch-all enum makes decode total over its width.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum Kind {
    A = 1,
    B = 2,
    #[catch_all]
    Other(u8),
}

/// Enum + scalar — total, 3 bytes.
#[bin(big)]
#[derive(Debug, Clone, PartialEq)]
struct Tagged {
    kind: Kind,
    value: u16,
}

/// A count-driven `Vec` whose length is derived (never stored).
#[bin(big)]
#[derive(Debug, Clone, PartialEq)]
struct Counted {
    #[br(temp)]
    #[bw(calc = self.items.len() as u8)]
    n: u8,
    #[br(count = n)]
    items: Vec<u16>,
}

/// A conditional `Option`.
#[bin(big)]
#[derive(Debug, Clone, PartialEq)]
struct Optional {
    present: u8,
    #[br(if(present != 0))]
    ext: Option<u16>,
}

/// A magic-prefixed message — decode of arbitrary bytes usually fails `BadMagic`.
#[bin(big, magic = 0xABCDu16)]
#[derive(Debug, Clone, PartialEq)]
struct Magic {
    body: u16,
}

proptest! {
    // --- 1. encode ∘ decode = id over random values --------------------------

    #[test]
    fn header_roundtrips(a in any::<u16>(), b in any::<u16>(), c in any::<u32>()) {
        let h = Header { a, b, c };
        let bytes = h.to_bytes().unwrap();
        prop_assert_eq!(bytes.len(), 8);
        prop_assert_eq!(Header::decode_exact(&bytes).unwrap(), h);
    }

    #[test]
    fn frame_roundtrips(tag in any::<u8>(), len in any::<u16>()) {
        let f = Frame { tag: u4::from_raw(tag), len: u12::from_raw(len) };
        prop_assert_eq!(Frame::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
    }

    #[test]
    fn tagged_roundtrips(kind in any::<u8>(), value in any::<u16>()) {
        let t = Tagged { kind: Kind::from(kind), value };
        prop_assert_eq!(Tagged::decode_exact(&t.to_bytes().unwrap()).unwrap(), t);
    }

    #[test]
    fn counted_roundtrips(items in prop::collection::vec(any::<u16>(), 0..50)) {
        let c = Counted { items: items.clone() };
        let decoded = Counted::decode_exact(&c.to_bytes().unwrap()).unwrap();
        prop_assert_eq!(decoded.items, items);
    }

    #[test]
    fn optional_roundtrips(present in any::<u8>(), v in any::<u16>()) {
        let ext = if present != 0 { Some(v) } else { None };
        let o = Optional { present, ext };
        prop_assert_eq!(Optional::decode_exact(&o.to_bytes().unwrap()).unwrap(), o);
    }

    #[test]
    fn magic_roundtrips(body in any::<u16>()) {
        let m = Magic { body };
        let bytes = m.to_bytes().unwrap();
        prop_assert_eq!(&bytes[..2], &[0xAB, 0xCD]); // the magic prefix
        prop_assert_eq!(Magic::decode_exact(&bytes).unwrap(), m);
    }

    // --- 2. decode of arbitrary bytes never panics ---------------------------
    // (proptest fails the test if any of these panics.)

    #[test]
    fn decode_arbitrary_bytes_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        let _ = Header::decode_exact(&bytes);
        let _ = Frame::decode_exact(&bytes);
        let _ = Tagged::decode_exact(&bytes);
        let _ = Counted::decode_exact(&bytes);
        let _ = Optional::decode_exact(&bytes);
        let _ = Magic::decode_exact(&bytes);
        // The other entry points must be equally robust.
        let _ = Header::peek(&bytes);
        let _ = Counted::peek(&bytes);
        let mut cursor = &bytes[..];
        let _ = Counted::decode(&mut cursor);
    }

    // --- 3. decode ∘ encode = id for fixed total parsers ---------------------
    // Any byte string of the right length decodes (the parser is total) and
    // re-encodes to exactly the same bytes (a bijection).

    #[test]
    fn header_is_a_bijection(bytes in any::<[u8; 8]>()) {
        let h = Header::decode_exact(&bytes).unwrap();
        prop_assert_eq!(h.to_bytes().unwrap(), bytes.to_vec());
    }

    #[test]
    fn frame_is_a_bijection(bytes in any::<[u8; 2]>()) {
        let f = Frame::decode_exact(&bytes).unwrap();
        prop_assert_eq!(f.to_bytes().unwrap(), bytes.to_vec());
    }

    #[test]
    fn tagged_is_a_bijection(bytes in any::<[u8; 3]>()) {
        let t = Tagged::decode_exact(&bytes).unwrap();
        prop_assert_eq!(t.to_bytes().unwrap(), bytes.to_vec());
    }
}
