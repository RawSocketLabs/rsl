//! The message-level **endian × bit-order** cross-product. `bin_byte_order.rs` covers
//! `big`/`little` and `bitstream_bitorder.rs` covers `msb`/`lsb`, but each axis alone —
//! this pins the 2×2 combination on one shape that is sensitive to *both*: a sub-byte
//! nibble pair (bit-order sensitive) followed by a `u16` word (byte-order sensitive).
//!
//! Golden bytes for the `lsb` combos are intentionally not hand-asserted (the packing
//! is subtle and already golden-tested per-axis); instead each combo must round-trip,
//! and flipping *either* axis must change the wire — proving the two options compose
//! independently rather than aliasing.

use bnb::{bin, u4};

// Four identical shapes, one per (endian, bit-order) corner. Defaults are big + msb.
#[bin(big, bit_order = msb)]
#[derive(Debug, PartialEq, Clone)]
struct BeMsb {
    hi: u4,
    lo: u4,
    word: u16,
}

#[bin(big, bit_order = lsb)]
#[derive(Debug, PartialEq, Clone)]
struct BeLsb {
    hi: u4,
    lo: u4,
    word: u16,
}

#[bin(little, bit_order = msb)]
#[derive(Debug, PartialEq, Clone)]
struct LeMsb {
    hi: u4,
    lo: u4,
    word: u16,
}

#[bin(little, bit_order = lsb)]
#[derive(Debug, PartialEq, Clone)]
struct LeLsb {
    hi: u4,
    lo: u4,
    word: u16,
}

// Distinct nibbles (hi != lo) and a word with distinct bytes (0x12 != 0x34), so that
// flipping either axis is guaranteed observable on the wire.
const HI: u8 = 0xA;
const LO: u8 = 0xB;
const WORD: u16 = 0x1234;

/// Encode the one shared logical value at type `$T`, returning its wire bytes.
macro_rules! enc {
    ($T:ident) => {
        $T {
            hi: u4::new(HI),
            lo: u4::new(LO),
            word: WORD,
        }
        .to_bytes()
        .unwrap()
    };
}

#[test]
fn every_combo_round_trips() {
    macro_rules! assert_round_trip {
        ($T:ident) => {{
            let v = $T {
                hi: u4::new(HI),
                lo: u4::new(LO),
                word: WORD,
            };
            assert_eq!($T::decode_exact(&v.to_bytes().unwrap()).unwrap(), v);
        }};
    }
    assert_round_trip!(BeMsb);
    assert_round_trip!(BeLsb);
    assert_round_trip!(LeMsb);
    assert_round_trip!(LeLsb);

    // All four messages are exactly 24 bits = 3 bytes.
    assert_eq!(enc!(BeMsb).len(), 3);
}

#[test]
fn flipping_byte_order_changes_the_word_bytes() {
    // Same bit-order (msb) ⇒ the nibble byte matches, but the word bytes swap.
    assert_ne!(
        enc!(BeMsb),
        enc!(LeMsb),
        "big vs little must differ on the wire (the u16 word)"
    );
}

#[test]
fn flipping_bit_order_changes_the_nibble_packing() {
    // Same byte-order (big) ⇒ the word bytes match, but the hi/lo nibble packing flips.
    assert_ne!(
        enc!(BeMsb),
        enc!(BeLsb),
        "msb vs lsb must differ on the wire (the nibble pair)"
    );
}

#[test]
fn all_four_corners_are_pairwise_distinct() {
    let encs = [enc!(BeMsb), enc!(BeLsb), enc!(LeMsb), enc!(LeLsb)];
    for i in 0..encs.len() {
        for j in (i + 1)..encs.len() {
            assert_ne!(
                encs[i], encs[j],
                "corners {i} and {j} alias — an axis isn't composing independently"
            );
        }
    }
}
