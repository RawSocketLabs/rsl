//! LSB-first bit order (ROADMAP Phase 1, chunk F): a struct marked
//! `#[bit_stream(bit_order = lsb)]` reads/writes least-significant-bit-first;
//! MSB-first is the default. Per-struct only (mixed order via nesting is Phase 2).

use bits::{BitDecode, BitEncode, u4, u12};

#[derive(BitDecode, BitEncode, Debug, PartialEq, Eq, Clone, Copy)]
struct MsbWord {
    a: u4,
    b: u12,
}

#[derive(BitDecode, BitEncode, Debug, PartialEq, Eq, Clone, Copy)]
#[bit_stream(bit_order = lsb)]
struct LsbWord {
    a: u4,
    b: u12,
}

#[test]
fn lsb_round_trips() {
    let v = LsbWord {
        a: u4::new(0xA),
        b: u12::new(0xBCD),
    };
    let bytes = v.to_bytes().unwrap();
    assert_eq!(LsbWord::decode_exact(&bytes).unwrap(), v);
}

#[test]
fn lsb_and_msb_differ_on_the_wire() {
    let msb = MsbWord {
        a: u4::new(0xA),
        b: u12::new(0xBCD),
    };
    let lsb = LsbWord {
        a: u4::new(0xA),
        b: u12::new(0xBCD),
    };
    // Same logical fields, opposite bit order ⇒ different bytes.
    assert_ne!(msb.to_bytes().unwrap(), lsb.to_bytes().unwrap());
}

#[test]
fn lsb_packs_the_first_field_into_low_bits() {
    // a = 0xA in the low nibble of byte 0 (LSB-first: value bit k -> byte bit k);
    // MSB-first would put 0xA in the high nibble instead.
    let lsb = LsbWord {
        a: u4::new(0xA),
        b: u12::new(0),
    };
    assert_eq!(lsb.to_bytes().unwrap()[0] & 0x0F, 0xA);

    let msb = MsbWord {
        a: u4::new(0xA),
        b: u12::new(0),
    };
    assert_eq!(msb.to_bytes().unwrap()[0] >> 4, 0xA);
}
