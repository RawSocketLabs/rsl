//! Coverage-guided promotion of the `decode_arbitrary_bytes_never_panics` proptest in
//! `bnb/tests/fuzz_roundtrip.rs`. The dual-use safety contract: a parser fed
//! hostile/garbage bytes returns `Ok` or `Err` — **never panics, never reads out of
//! bounds, never loops unboundedly**. libFuzzer drives the byte string; ASan/UBSan
//! (linked by `cargo fuzz`) catch memory/UB the `#[should_panic]`-free proptest can't.
//!
//! Run: `cargo +nightly fuzz run decode`.
#![no_main]

use bnb::{
    BitEnum, BitError, BitOrder, BufSource, ByteOrder, Layout, NormalizeEnumAliases, Source,
    StreamBitReader, bin, prelude::*, u4, u12,
};
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

/// A pointer-following message: it ends wherever `off` sends the cursor, possibly before
/// its own start — the shape that must not stall a slice walk.
#[bin(big)]
#[derive(Debug, Clone, PartialEq)]
struct Jump {
    off: u8,
    #[br(seek = off.bytes())]
    target: u8,
}

/// Contains itself through a counted `Vec`: each nonzero byte opens one more level.
#[bin]
#[derive(Debug)]
struct Node {
    n: u8,
    #[br(count = n)]
    kids: Vec<Node>,
}

/// The header again, least significant byte first.
#[bin(little)]
#[derive(Debug, Clone, PartialEq)]
struct HeaderLe {
    a: u16,
    b: u16,
    c: u32,
}

/// The sub-byte frame again, filled from the low bit of each byte.
#[bin(bits = lsb, bytes = little)]
#[derive(Debug, Clone, PartialEq)]
struct FrameLsb {
    tag: u4,
    len: u12,
}

/// Every field at most a byte or whole bytes on a byte boundary: the shape a fixed-length
/// message decodes from one window of its whole length.
#[bin(big)]
#[derive(Debug, Clone, PartialEq)]
struct Windowed {
    high: u4,
    low: u4,
    word: u16,
    tail: [u8; 3],
}

/// A LEB128 length through the crate's own field codec, then a byte.
#[bin(big)]
#[derive(Debug, Clone, PartialEq)]
struct Varint {
    #[br(parse_with = bnb::codecs::leb128::parse)]
    #[bw(write_with = bnb::codecs::leb128::write)]
    n: u64,
    tail: u8,
}

/// A one-byte region length, then a counted message that must fill the region exactly.
fn in_region<S: Source>(r: &mut S) -> Result<Counted, BitError> {
    let len = usize::from(r.read::<u8>()?);
    let mut region = r.limit_bytes(len)?;
    let value = <Counted as bnb::BitDecode>::bit_decode(&mut region)?;
    region.finish()?;
    Ok(value)
}

/// A message behind a window that its reads cannot cross.
#[bin(big, read_only)]
#[derive(Debug, PartialEq)]
struct Region {
    #[br(parse_with = in_region)]
    inner: Counted,
}

const BIG: Layout = Layout {
    bit: BitOrder::Msb,
    byte: ByteOrder::Big,
};
const LITTLE: Layout = Layout {
    bit: BitOrder::Msb,
    byte: ByteOrder::Little,
};
const LSB_LITTLE: Layout = Layout {
    bit: BitOrder::Lsb,
    byte: ByteOrder::Little,
};

/// One message off the front of `data`, three ways: from the slice, from a forward reader
/// that holds nothing, and from one that retains what it read. The same message or no
/// message from all three; and the bytes the slice decode says it used decode alone to
/// that message.
macro_rules! one_message_whichever_reader {
    ($data:expr, $($ty:ty => $layout:expr),* $(,)?) => {$(
        let slice = <$ty>::decode_prefix($data);
        let forward = <$ty>::decode(&mut StreamBitReader::with_layout($data, $layout));
        let retained = <$ty>::decode(&mut BufSource::with_capacity_and_layout(
            $data,
            usize::MAX,
            $layout,
        ));
        let value = slice.as_ref().ok().map(|(value, _)| value);
        assert_eq!(value, forward.as_ref().ok(), concat!(stringify!($ty), ": forward reader"));
        assert_eq!(value, retained.as_ref().ok(), concat!(stringify!($ty), ": retaining reader"));
        if let Ok((value, used)) = &slice {
            assert_eq!(
                <$ty>::decode_exact(&$data[..*used]).as_ref().ok(),
                Some(value),
                concat!(stringify!($ty), ": its own prefix")
            );
        }
    )*};
}

fuzz_target!(|data: &[u8]| {
    if let Some(&raw) = data.first() {
        let mut built = Tagged::builder()
            .kind(Kind::Other(raw))
            .value(7)
            .build()
            .unwrap();
        assert_eq!(built.kind, Kind::from(raw));
        assert_eq!(built.to_bytes().unwrap(), [raw, 0, 7]);
        let normalized = built.clone();
        built.normalize_enum_aliases();
        assert_eq!(built, normalized);
    }
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
    // A walk decodes at most one message per input bit, plus the one that stops it —
    // even when a message seeks backward.
    let bound = data.len() * 8 + 1;
    let _ = Jump::decode_all(data);
    assert!(
        Jump::decode_iter(data).take(bound + 1).count() <= bound,
        "a slice walk must terminate"
    );

    // A recursive type ends in an error, never a stack overflow: past the depth budget
    // (129 leading nonzero bytes open 129 nested `Vec`s) it is exactly `TooDeep`.
    let deep = usize::try_from(bnb::MAX_DECODE_DEPTH).unwrap();
    let node = Node::decode_exact(data);
    if data.len() > deep && data[..=deep].iter().all(|&b| b != 0) {
        assert_eq!(node.unwrap_err().kind, bnb::ErrorKind::TooDeep);
    }

    // Property 4 — a message is the same message whichever reader it comes through, in
    // every layout, behind a window, and through a field codec.
    one_message_whichever_reader!(
        data,
        Header => BIG,
        HeaderLe => LITTLE,
        Frame => BIG,
        FrameLsb => LSB_LITTLE,
        Windowed => BIG,
        Tagged => BIG,
        Counted => BIG,
        Optional => BIG,
        Magic => BIG,
        Varint => BIG,
        Region => BIG,
    );
    for (bijection, bytes) in [
        (HeaderLe::decode_exact(data).map(|v| v.to_bytes().unwrap()), "HeaderLe"),
        (FrameLsb::decode_exact(data).map(|v| v.to_bytes().unwrap()), "FrameLsb"),
        (Windowed::decode_exact(data).map(|v| v.to_bytes().unwrap()), "Windowed"),
    ] {
        if let Ok(again) = bijection {
            assert_eq!(again, data, "{bytes} is not a bijection");
        }
    }

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
