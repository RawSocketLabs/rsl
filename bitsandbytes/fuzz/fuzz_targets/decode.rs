//! Coverage-guided promotion of the `decode_arbitrary_bytes_never_panics` proptest in
//! `bnb/tests/fuzz_roundtrip.rs`. The dual-use safety contract: a parser fed
//! hostile/garbage bytes returns `Ok` or `Err` — **never panics, never reads out of
//! bounds, never loops unboundedly**. libFuzzer drives the byte string; ASan/UBSan
//! (linked by `cargo fuzz`) catch memory/UB the `#[should_panic]`-free proptest can't.
//!
//! Run: `cargo +nightly fuzz run decode`.
#![no_main]

use bnb::{BitEnum, bin, u4, u12};
use libfuzzer_sys::fuzz_target;

// --- shapes (mirror tests/fuzz_roundtrip.rs) ----------------------------------

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

/// A count-driven `Vec` whose length is derived (never stored) — the attacker-
/// controlled-`count` push guard lives here.
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

fuzz_target!(|data: &[u8]| {
    // Property 2 — decode of arbitrary bytes never panics. Every entry point on
    // every shape must be equally robust.
    let _ = Header::decode_exact(data);
    let _ = Frame::decode_exact(data);
    let _ = Tagged::decode_exact(data);
    let _ = Counted::decode_exact(data);
    let _ = Optional::decode_exact(data);
    let _ = Magic::decode_exact(data);
    let _ = Header::peek(data);
    let _ = Counted::peek(data);
    let _ = Counted::decode_all(data);
    let _ = Counted::decode_iter(data).count();

    // Property 3 — decode ∘ encode = id for the fixed-length total parsers. Any
    // byte string of the right length decodes (the parser is total) and re-encodes
    // to exactly those bytes (a bijection); an asymmetry is a real bug.
    if let Ok(h) = Header::decode_exact(data) {
        assert_eq!(h.to_bytes().unwrap(), data, "Header is not a bijection");
    }
    if let Ok(f) = Frame::decode_exact(data) {
        assert_eq!(f.to_bytes().unwrap(), data, "Frame is not a bijection");
    }
    if let Ok(t) = Tagged::decode_exact(data) {
        assert_eq!(t.to_bytes().unwrap(), data, "Tagged is not a bijection");
    }
});
