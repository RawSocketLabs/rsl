//! **can_signals** — arbitrary-width fields packed **LSB-first** (`bit_order = lsb`).
//!
//! `arbitrary_width` and `ais` both pack MSB-first (big-endian, the network convention). The other
//! common convention puts the *first* field in the **low** bits — CAN/DBC "Intel" signals, SMB,
//! and many device registers pack this way. This models a small CAN-style engine frame:
//! `mode` (3 bits) │ `gear` (4) │ `rpm` (14) │ `mil` (1) = 22 bits — not byte-aligned —
//! little-endian bytes, LSB-first bits.
//!
//! Run with: `cargo run -p bitsandbytes --example can_signals`

use bnb::{BitEnum, bin, u3, u4, u14};

/// Drive mode (3 bits); unlisted codes are kept by `#[catch_all]`.
#[derive(BitEnum, Copy, Clone, Debug, PartialEq, Eq)]
#[bit_enum(u3)]
#[repr(u8)]
enum DriveMode {
    Eco = 0,
    Normal = 1,
    Sport = 2,
    #[catch_all]
    Other(u3),
}

/// A CAN-style engine status frame — 22 bits, little-endian / LSB-first ("Intel" signals).
#[bin(little, bit_order = lsb)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct EngineFrame {
    mode: DriveMode, // lands in the LOW 3 bits of byte 0 (LSB-first)
    gear: u4,
    rpm: u14,  // 0..=16383
    mil: bool, // malfunction indicator lamp (one bit)
}

fn main() {
    let frame = EngineFrame {
        mode: DriveMode::Sport,
        gear: u4::new(4),
        rpm: u14::new(3500),
        mil: false,
    };
    let bytes = frame.to_bytes().unwrap();
    println!("encoded: {} bytes  {bytes:02x?}", bytes.len()); // 22 bits -> 3 bytes (2 pad)
    assert_eq!(EngineFrame::decode_exact(&bytes).unwrap(), frame);

    // `little` + `lsb` IS the DBC-Intel layout, byte-identically: signal value v at
    // start-bit S occupies `raw |= v << S` of a little-endian integer. Prove it against
    // the reference formula (mode@0, gear@3, rpm@7, mil@21):
    let raw: u32 = 2 | (4 << 3) | (3500 << 7); // Sport=2, gear=4, rpm=3500, mil=0
    assert_eq!(bytes, raw.to_le_bytes()[..3]);

    // an unlisted drive-mode code is preserved by #[catch_all], not rejected (dual-use)
    let unknown = EngineFrame {
        mode: DriveMode::Other(u3::new(6)),
        ..frame.clone()
    };
    assert_eq!(
        EngineFrame::decode_exact(&unknown.to_bytes().unwrap()).unwrap(),
        unknown
    );

    println!("{frame:#?}");
    println!("all checks passed");
}
