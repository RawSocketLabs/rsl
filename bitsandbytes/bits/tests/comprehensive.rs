//! Exhaustive behavioral coverage of the codec: every backing width, both bit
//! orders, every field-width form, masking/overflow, deep nesting, byte-order,
//! enum exhaustiveness/catch-all, and error paths. Codec-only, so it runs with
//! and without the `binrw` feature.

use bits::{bitfield, u2, u3, u4, u5, u12, u24, BitEnum, Bits};

// ---------------------------------------------------------------------------
// Every backing width packs/unpacks correctly.
// ---------------------------------------------------------------------------

#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct B8 { hi: u4, lo: u4 }

#[bitfield(u32, bits = msb, bytes = be)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct B32 { a: u8, b: u12, c: u12 }

#[bitfield(u64, bits = msb)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct B64 { tag: u4, payload: bits::u60 }

#[bitfield(u128, bits = msb)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct B128 { version: u4, rest: bits::u124 }

#[test]
fn all_backings_round_trip() {
    let b8 = B8::new().with_hi(u4::new(0xA)).with_lo(u4::new(0x5));
    assert_eq!(b8.raw(), 0xA5);
    assert_eq!(b8.hi(), u4::new(0xA));

    let b32 = B32::new().with_a(0xFF).with_b(u12::new(0xABC)).with_c(u12::new(0xDEF));
    // a in bits 24..=31, b in 12..=23, c in 0..=11.
    assert_eq!(b32.raw(), 0xFF_ABC_DEF);
    assert_eq!(b32.to_be_bytes(), [0xFF, 0xAB, 0xCD, 0xEF]);

    let b64 = B64::new().with_tag(u4::new(0x9)).with_payload(bits::u60::new(0x123));
    assert_eq!(b64.tag(), u4::new(9));
    assert_eq!(b64.payload(), bits::u60::new(0x123));

    let b128 = B128::new().with_version(u4::new(6));
    assert_eq!(b128.version(), u4::new(6));
    assert_eq!(b128.raw() >> 124, 6);
}

// ---------------------------------------------------------------------------
// MSB vs LSB are mirror images of each other.
// ---------------------------------------------------------------------------

#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy)]
struct Msb { first: u3, second: u5 }

#[bitfield(u8, bits = lsb)]
#[derive(Clone, Copy)]
struct Lsb { first: u3, second: u5 }

#[test]
fn bit_order_is_mirrored() {
    // MSB: `first` (0b101) lands in the high 3 bits (offset 5) -> 0b1010_0000.
    let m = Msb::new().with_first(u3::new(0b101));
    assert_eq!(m.raw(), 0b1010_0000);

    // LSB: the same value lands in the low 3 bits -> 0b0000_0101.
    let l = Lsb::new().with_first(u3::new(0b101));
    assert_eq!(l.raw(), 0b0000_0101);
}

// ---------------------------------------------------------------------------
// The three width forms agree on the same layout.
// ---------------------------------------------------------------------------

#[bitfield(u16, bits = msb)]
#[derive(Clone, Copy)]
struct Inferred { a: u5, b: bits::u7, c: u4 }

#[bitfield(u16, bits = msb)]
#[derive(Clone, Copy)]
struct Widths {
    #[bits(5)] a: u8,
    #[bits(7)] b: u8,
    #[bits(4)] c: u8,
}

#[bitfield(u16)]
#[derive(Clone, Copy)]
struct Ranges {
    #[bits(11..=15)] a: u8,
    #[bits(4..=10)] b: u8,
    #[bits(0..=3)] c: u8,
}

#[test]
fn width_forms_produce_identical_layouts() {
    let i = Inferred::new().with_a(u5::new(2)).with_c(u4::new(3));
    let w = Widths::new().with_a(2).with_c(3);
    let r = Ranges::new().with_a(2).with_c(3);
    assert_eq!(i.raw(), 0x1003);
    assert_eq!(w.raw(), 0x1003);
    assert_eq!(r.raw(), 0x1003);
}

// ---------------------------------------------------------------------------
// Masking: oversized values are masked to the field width, never bleed.
// ---------------------------------------------------------------------------

#[test]
fn field_setters_mask_to_width() {
    // `with_` masks the incoming value, and never disturbs neighbours.
    let b = B8::new().with_hi(u4::new(0xF)).with_lo(u4::new(0xF));
    assert_eq!(b.raw(), 0xFF);
    // Setting hi again does not touch lo.
    let b = b.with_hi(u4::new(0x3));
    assert_eq!(b.raw(), 0x3F);

    // from_raw stores the whole backing; getters mask out their slice.
    let b = B8::from_raw(0xFF);
    assert_eq!(b.hi(), u4::new(0xF));
    assert_eq!(b.lo(), u4::new(0xF));
}

// ---------------------------------------------------------------------------
// Reserved / padding: declared width may be < backing width.
// ---------------------------------------------------------------------------

#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy)]
struct Partial { flag: bool, value: u4 } // 5 of 8 bits used

#[test]
fn partial_width_leaves_high_bits_clear() {
    let p = Partial::new().with_flag(true).with_value(u4::new(0xF));
    // 5 bits: flag at offset 4, value at 0..=3.
    assert_eq!(p.raw(), 0b0001_1111);
    assert_eq!(<Partial as Bits>::BITS, 5);
}

// ---------------------------------------------------------------------------
// Deep nesting: bitfield in bitfield in bitfield.
// ---------------------------------------------------------------------------

#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Inner { x: u2, y: u2 } // 4 bits

#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Middle { inner: Inner, z: u4 } // 8 bits

#[bitfield(u16, bits = msb)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Outer { middle: Middle, w: u8 } // 16 bits

#[test]
fn three_levels_of_nesting_compose() {
    let v = Outer::new()
        .with_middle(
            Middle::new()
                .with_inner(Inner::new().with_x(u2::new(0b11)).with_y(u2::new(0b01)))
                .with_z(u4::new(0xA)),
        )
        .with_w(0x42);
    // middle occupies the high byte: inner(4) | z(4) = 1101_1010 = 0xDA.
    assert_eq!(v.to_be_bytes(), [0xDA, 0x42]);
    assert_eq!(v.middle().inner().x(), u2::new(0b11));
    assert_eq!(v.middle().z(), u4::new(0xA));
    assert_eq!(v.w(), 0x42);
}

// ---------------------------------------------------------------------------
// Byte order matrix.
// ---------------------------------------------------------------------------

#[bitfield(u32, bytes = be)]
#[derive(Clone, Copy)]
struct BeWord { #[bits(0..=31)] v: u32 }

#[bitfield(u32, bytes = le)]
#[derive(Clone, Copy)]
struct LeWord { #[bits(0..=31)] v: u32 }

#[test]
fn byte_order_controls_serialized_bytes() {
    let be = BeWord::from_raw(0x01020304);
    let le = LeWord::from_raw(0x01020304);
    assert_eq!(be.to_be_bytes(), [0x01, 0x02, 0x03, 0x04]);
    assert_eq!(le.to_le_bytes(), [0x04, 0x03, 0x02, 0x01]);
    // The inherent *_bytes helpers are order-agnostic; the declared order is the
    // codec default (used by binrw and the `Bitfield` seam).
    use bits::{Bitfield, ByteOrder};
    assert_eq!(<BeWord as Bitfield>::BYTE_ORDER, ByteOrder::Big);
    assert_eq!(<LeWord as Bitfield>::BYTE_ORDER, ByteOrder::Little);
}

// ---------------------------------------------------------------------------
// Enums: exhaustive, catch-all, and the contract for neither.
// ---------------------------------------------------------------------------

#[derive(BitEnum, Clone, Copy, PartialEq, Eq, Debug)]
#[bit_enum(u2)]
enum Exhaustive { Zero, One, Two, Three } // all 4 values of a 2-bit field

#[derive(BitEnum, Clone, Copy, PartialEq, Eq, Debug)]
#[bit_enum(u2)]
enum WithCatch { Zero, One, #[catch_all] Rest(u2) }

#[derive(BitEnum, Clone, Copy, PartialEq, Eq, Debug)]
#[bit_enum(u2)]
enum Incomplete { Zero, One } // 2 of 4 named, NO catch-all: a stated-exhaustive lie

#[test]
fn exhaustive_enum_round_trips_every_value() {
    for v in 0u128..4 {
        assert_eq!(Exhaustive::from_bits(v).into_bits(), v);
    }
}

#[test]
fn catch_all_preserves_every_unrepresented_value() {
    assert_eq!(WithCatch::from_bits(0), WithCatch::Zero);
    assert_eq!(WithCatch::from_bits(2), WithCatch::Rest(u2::new(2)));
    assert_eq!(WithCatch::from_bits(3), WithCatch::Rest(u2::new(3)));
    assert_eq!(WithCatch::Rest(u2::new(3)).into_bits(), 3);
}

#[test]
#[should_panic(expected = "no variant for discriminant")]
fn non_exhaustive_without_catch_all_panics_on_the_gap() {
    // Documented contract: omitting #[catch_all] asserts exhaustiveness; a
    // value with no variant is a declaration bug and surfaces loudly.
    let _ = Incomplete::from_bits(2);
}

// ---------------------------------------------------------------------------
// UInt boundaries and error paths.
// ---------------------------------------------------------------------------

#[test]
fn uint_boundaries() {
    assert_eq!(u4::MIN.value(), 0);
    assert_eq!(u4::MAX.value(), 15);
    assert_eq!(u24::MAX.value(), 0xFF_FFFF);
    assert!(u4::try_new(15).is_ok());
    assert!(u4::try_new(16).is_err());
}

#[test]
fn error_messages_are_informative() {
    let e = u4::try_new(99).unwrap_err();
    assert_eq!(e, bits::Error::ValueTooLarge { value: 99, bits: 4 });
    assert_eq!(e.to_string(), "value 99 does not fit in 4 bits");
}
