//! **endianness** — bit order and byte order are two independent knobs.
//!
//! `bnb` separates *bit order* (`msb`/`lsb` — does the first field land in the high or low bits of
//! a word) from *byte order* (`big`/`little` — how a byte-multiple value serializes). They compose
//! **independently**: flipping one never affects the other. This shows both at the `#[bin]`
//! message level and at the low-level `BitReader`/`BitWriter` cursor (via an explicit `Layout`).
//!
//! Run with: `cargo run -p bitsandbytes --example endianness`

use bnb::{BitOrder, BitReader, BitWriter, ByteOrder, Layout, bin, u4};

// --- byte order: how a multi-byte value serializes ------------------------------
#[bin(big)]
#[derive(Debug, PartialEq)]
struct WordBe {
    v: u32,
}
#[bin(little)]
#[derive(Debug, PartialEq)]
struct WordLe {
    v: u32,
}

// --- bit order: how a sub-byte field packs --------------------------------------
#[bin(big, bit_order = msb)]
#[derive(Debug, PartialEq)]
struct NibblesMsb {
    a: u4,
    b: u4,
}
#[bin(big, bit_order = lsb)]
#[derive(Debug, PartialEq)]
struct NibblesLsb {
    a: u4,
    b: u4,
}

// --- both axes on one shape: a nibble pair (bit-order sensitive) + a u16 word
//     (byte-order sensitive), one struct per (bit × byte) corner --------------------
#[bin(big, bit_order = msb)]
#[derive(Debug, PartialEq)]
struct BeMsb {
    hi: u4,
    lo: u4,
    word: u16,
}
#[bin(little, bit_order = msb)]
#[derive(Debug, PartialEq)]
struct LeMsb {
    hi: u4,
    lo: u4,
    word: u16,
}
#[bin(big, bit_order = lsb)]
#[derive(Debug, PartialEq)]
struct BeLsb {
    hi: u4,
    lo: u4,
    word: u16,
}
#[bin(little, bit_order = lsb)]
#[derive(Debug, PartialEq)]
struct LeLsb {
    hi: u4,
    lo: u4,
    word: u16,
}

fn main() {
    // (1) BYTE order swaps the bytes of a multi-byte value; the logical value is unchanged.
    assert_eq!(
        WordBe { v: 0x1234_5678 }.to_bytes().unwrap(),
        [0x12, 0x34, 0x56, 0x78]
    );
    assert_eq!(
        WordLe { v: 0x1234_5678 }.to_bytes().unwrap(),
        [0x78, 0x56, 0x34, 0x12]
    );
    println!("byte order:  big = 12 34 56 78    little = 78 56 34 12   (same u32)");

    // (2) BIT order flips which nibble is high; a sub-byte field is unaffected by byte order.
    let (a, b) = (u4::new(0xA), u4::new(0xB));
    assert_eq!(NibblesMsb { a, b }.to_bytes().unwrap(), [0xAB]);
    assert_eq!(NibblesLsb { a, b }.to_bytes().unwrap(), [0xBA]);
    println!("bit order:   msb = AB              lsb = BA              (same nibbles a=A, b=B)");

    // (3) The two are INDEPENDENT: all four corners of (bit × byte) are distinct, and each
    //     round-trips through its own decoder.
    macro_rules! corner {
        ($T:ident) => {{
            let v = $T {
                hi: u4::new(0xA),
                lo: u4::new(0xB),
                word: 0x1234,
            };
            let bytes = v.to_bytes().unwrap();
            assert_eq!($T::decode_exact(&bytes).unwrap(), v); // round-trips
            bytes
        }};
    }
    let corners = [
        corner!(BeMsb),
        corner!(LeMsb),
        corner!(BeLsb),
        corner!(LeLsb),
    ];
    for i in 0..corners.len() {
        for j in (i + 1)..corners.len() {
            assert_ne!(corners[i], corners[j], "an axis is aliasing the other");
        }
    }
    println!("independent: all four (bit × byte) corners are distinct and round-trip");

    // (4) The same two knobs at the low-level cursor, via an explicit `Layout`.
    let layout = Layout {
        bit: BitOrder::Msb,
        byte: ByteOrder::Little,
    };
    let mut w = BitWriter::with_layout(layout);
    w.write(0x1234u16).unwrap();
    assert_eq!(w.into_bytes(), [0x34, 0x12]); // little-endian on the wire
    let mut r = BitReader::with_layout(&[0x34, 0x12], layout);
    assert_eq!(r.read::<u16>().unwrap(), 0x1234); // and back
    println!("low-level:   BitWriter/BitReader honor an explicit Layout (msb, little) for a u16");

    println!("all checks passed");
}
