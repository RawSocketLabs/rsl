//! Walk-through for the "u28 = u17 + u11 in a u32 backing" question: can we
//! place non-byte-aligned fields, with control over where the bits land, and
//! does the bit-shifting math stay correct when they straddle byte boundaries?
//!
//! Two distinct, complementary mechanisms — and they compose:
//!   1. `#[bitfield]` packs the fields into a backing integer (the `bitbybit`
//!      equivalent). Two placement styles:
//!        - auto-layout (widths)  -> `WIDTH = Σ fields`        (28 here)
//!        - manual `#[bits(A..=B)]` ranges -> `WIDTH = backing` (32 here, gaps included)
//!   2. `#[bin]` (Phase 2 stream codec) writes fields consecutively to a bit
//!      cursor; a `#[bitfield]` nests into it as a `Bits` leaf contributing `WIDTH`
//!      bits. So you never hand-shift — declaration order + bit order + the chosen
//!      placement style fully determine the layout.

use bnb::{Bitfield, Bits, bin, bitfield, u4, u11, u17};

// ---------------------------------------------------------------------------
// (1a) Auto-layout: WIDTH = 17 + 11 = 28. MSB-first => first field in the high
// bits. No need to say "where the bits land" — order + msb does it.
// ---------------------------------------------------------------------------
#[bitfield(u32, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AutoWord {
    hi: u17, // first field -> high 17 bits of the 28
    lo: u11, // -> low 11 bits
}

#[test]
fn auto_width_is_field_sum() {
    assert_eq!(<AutoWord as Bitfield>::WIDTH, 28);
    assert_eq!(<AutoWord as Bits>::BITS, 28); // what it contributes when nested

    // hi occupies bits 27..=11, lo occupies bits 10..=0 of the 28-bit value.
    let w = AutoWord::new()
        .with_hi(u17::new(0x1FFFF))
        .with_lo(u11::new(0));
    assert_eq!(w.raw(), 0x1FFFF << 11);
    let w = AutoWord::new()
        .with_hi(u17::new(0))
        .with_lo(u11::new(0x7FF));
    assert_eq!(w.raw(), 0x7FF);

    // Round-trip through the backing bytes (the shift/mask is the macro's job).
    let w = AutoWord::new()
        .with_hi(u17::new(0x15555))
        .with_lo(u11::new(0x2AA));
    assert_eq!(AutoWord::from_be_bytes(w.to_be_bytes()), w);
}

// ---------------------------------------------------------------------------
// (1b) Manual ranges: absolute placement, WIDTH = backing = 32. Here we leave an
// intentional 4-bit gap (bits 14..=11) — exactly the bitbybit "specify which bits
// are filled" use case, including non-contiguous fields.
// ---------------------------------------------------------------------------
#[bitfield(u32, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlacedWord {
    #[bits(15..=31)] // top 17 bits (offset = low end = 15)
    hi: u17,
    #[bits(0..=10)] // low 11 bits
    lo: u11,
    // bits 11..=14 are an intentional reserved gap (transmitted as 0).
}

#[test]
fn manual_ranges_place_bits_with_a_gap() {
    assert_eq!(<PlacedWord as Bitfield>::WIDTH, 32); // full backing on the wire

    let w = PlacedWord::new().with_hi(u17::new(1)).with_lo(u11::new(1));
    assert_eq!(w.raw(), (1u32 << 15) | 1); // hi at bit 15, lo at bit 0

    // Even fully saturated, the 4-bit gap stays zero — the fields don't bleed.
    let full = PlacedWord::new()
        .with_hi(u17::new(0x1FFFF))
        .with_lo(u11::new(0x7FF));
    assert_eq!(full.raw(), 0xFFFF_87FF);
    assert_eq!(full.raw() & (0xF << 11), 0, "the reserved gap is untouched");
    assert_eq!(full.hi(), u17::new(0x1FFFF));
    assert_eq!(full.lo(), u11::new(0x7FF));
}

// ---------------------------------------------------------------------------
// (2) The composition that matters for Phase 2: a packed word nested in a #[bin]
// stream where the *message* is not byte-aligned (a 4-bit tag in front). The
// bitfield contributes its WIDTH (28) bits; the cursor straddles bytes for us.
// ---------------------------------------------------------------------------
#[bin]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Msg {
    tag: u4,
    word: AutoWord, // Bits leaf -> 28 bits; 4 + 28 = 32 bits = 4 bytes
}

#[test]
fn bitfield_nests_in_bin_stream() {
    let word = AutoWord::new()
        .with_hi(u17::new(0x1FFFF))
        .with_lo(u11::new(0x000));
    let m = Msg {
        tag: u4::new(0xF),
        word,
    };

    let bytes = m.to_bytes().unwrap();
    assert_eq!(bytes.len(), 4, "4 + 28 = 32 bits, no padding");
    // tag = 0xF (bits 0..=3, msb), then hi=all-ones (next 17 bits), then lo=0.
    // 1111 1_1111111 1111111_11 000...  -> 0xFF FF F8 00
    assert_eq!(bytes, [0xFF, 0xFF, 0xF8, 0x00]);

    assert_eq!(Msg::decode_exact(&bytes).unwrap(), m);
}
