//! `#[derive(BitEnum)]` in depth — the `num_enum` / `bitbybit::bitenum`
//! replacement. Shows exhaustive enums, catch-all (dual-use) enums, nesting in a
//! bitfield, and checked integer construction with error handling. Codec-only,
//! so it runs with or without binrw:
//!
//!   `cargo run -p bits --example enums`
//!   `cargo run -p bits --example enums --no-default-features`

use bits::{BitEnum, Bits, bitfield, u2, u4};

// An exhaustive 2-bit enum: all four values named, so no catch-all is needed.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
enum Ecn {
    NotEct,
    Ect1,
    Ect0,
    Ce,
}

// An ARP-style hardware-type enum: a catch-all preserves the long tail of IANA
// values we don't name (exactly the `num_enum(catch_all)` pattern). Because the
// width is byte-aligned, the derive also emits `From<HardwareType> for u16` and,
// thanks to the catch-all, an infallible `From<u16> for HardwareType` — so this
// one derive replaces a hand-written `#[binrw]` magic-byte enum *and* its
// `impl From<…> for u16` *and* the round-trips-every-discriminant test.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u16, bytes = be)]
#[repr(u16)]
enum HardwareType {
    Ethernet = 1,
    Ieee802 = 6,
    FrameRelay = 15,
    InfiniBand = 32,
    #[catch_all]
    Other(u16),
}

// No catch-all: the derive emits a *checked* `TryFrom<u8>` instead, rejecting
// values it can't name with a `bits::UnknownDiscriminant`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum Direction {
    Request = 1,
    Reply = 2,
}

// A 4-bit catch-all enum nested in an 8-bit bitfield.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum Op {
    Read,
    Write,
    #[catch_all]
    Vendor(u4),
}

#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, Debug)]
struct Command {
    op: Op,
    flags: u4,
}

fn main() {
    // Exhaustive: every value maps to a named variant.
    for raw in 0u128..4 {
        println!("ecn {raw} -> {:?}", Ecn::from_bits(raw));
    }

    // Catch-all: known values are named, unknown ones preserved losslessly.
    println!("hw 1  -> {:?}", HardwareType::from_bits(1)); // Ethernet
    println!("hw 99 -> {:?}", HardwareType::from_bits(99)); // Other(99)
    assert_eq!(HardwareType::from_bits(99), HardwareType::Other(99));
    assert_eq!(HardwareType::Other(99).into_bits(), 99); // round-trips

    // The num_enum-style `From` parity (byte-aligned + catch-all => both ways
    // infallible). No hand-written `impl From<…> for u16`, no round-trip test.
    assert_eq!(u16::from(HardwareType::Ethernet), 1);
    assert_eq!(HardwareType::from(99u16), HardwareType::Other(99));
    assert_eq!(u16::from(HardwareType::from(0x1234u16)), 0x1234); // total round-trip

    // No catch-all => a checked `TryFrom`, not a panic-on-unknown.
    assert_eq!(Direction::try_from(2u8), Ok(Direction::Reply));
    match Direction::try_from(7u8) {
        Ok(d) => println!("direction: {d:?}"),
        Err(e) => println!("rejected: {e}"), // "Direction has no variant for discriminant 7"
    }
    assert!(Direction::try_from(7u8).is_err());

    // Nested in a bitfield.
    let cmd = Command::new()
        .with_op(Op::Vendor(u4::new(0xC)))
        .with_flags(u4::new(0x5));
    println!(
        "command byte: {:#04x} (op={:?}, flags={})",
        cmd.raw(),
        cmd.op(),
        cmd.flags()
    );
    assert_eq!(cmd.op(), Op::Vendor(u4::new(0xC)));

    // Checked integer construction — errors instead of panicking.
    match u2::try_new(5) {
        Ok(v) => println!("ok: {v}"),
        Err(e) => println!("rejected: {e}"), // "value 5 does not fit in 2 bits"
    }
    assert!(u2::try_new(3).is_ok());
    assert!(u2::try_new(4).is_err());
}
