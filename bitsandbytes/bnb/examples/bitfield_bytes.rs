//! **bitfield_bytes** — a `#[bitfield]`'s declared byte order (`bytes = be|le`) drives
//! `to_bytes()`/`from_bytes()`, while `to_be_bytes`/`to_le_bytes` are the explicit override.
//!
//! The *same* logical value, declared big- vs little-endian, serializes to *different* wire bytes
//! through `to_bytes()` — the order-respecting path. The endianness-explicit methods ignore the
//! declaration; reach for them only to force a specific order regardless of how the type was
//! declared.
//!
//! Run with: `cargo run -p bitsandbytes --example bitfield_bytes`

use bnb::{bitfield, u4};

/// A 16-bit field packed MSB-first, declared **big-endian**.
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TagBe {
    hi: u4,
    mid: u8,
    lo: u4,
}

/// The same fields, declared **little-endian**.
#[bitfield(u16, bits = msb, bytes = le)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TagLe {
    hi: u4,
    mid: u8,
    lo: u4,
}

fn main() {
    let be = TagBe::new()
        .with_hi(u4::new(0xA))
        .with_mid(0xBC)
        .with_lo(u4::new(0xD));
    let le = TagLe::new()
        .with_hi(u4::new(0xA))
        .with_mid(0xBC)
        .with_lo(u4::new(0xD));

    // Same logical value (raw = 0xABCD), packed identically (both MSB-first)...
    assert_eq!(be.raw(), 0xABCD);
    assert_eq!(le.raw(), 0xABCD);

    // ...but `to_bytes()` serializes in each type's DECLARED byte order:
    assert_eq!(be.to_bytes(), [0xAB, 0xCD]); // big-endian
    assert_eq!(le.to_bytes(), [0xCD, 0xAB]); // little-endian
    println!(
        "to_bytes() honors the declaration: be -> {:02X?}, le -> {:02X?}",
        be.to_bytes(),
        le.to_bytes()
    );

    // `from_bytes()` is the inverse, also in the declared order — a clean round-trip:
    assert_eq!(TagBe::from_bytes(be.to_bytes()), be);
    assert_eq!(TagLe::from_bytes(le.to_bytes()), le);

    // The endianness-explicit methods IGNORE the declaration — use them only to override:
    assert_eq!(le.to_be_bytes(), [0xAB, 0xCD]); // force big, though declared little
    assert_eq!(be.to_le_bytes(), [0xCD, 0xAB]); // force little, though declared big
    println!("to_be_bytes/to_le_bytes override the declaration (explicit endianness)");

    println!("all checks passed");
}
