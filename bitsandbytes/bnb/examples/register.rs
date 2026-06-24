//! **register** — `#[reserved]` / `#[reserved_with]` bits + `pad` positioning in one fixed-layout
//! record (a hardware-style control register): two reserved fields — a must-be-zero nibble and a
//! must-be-one nibble — around a padded value word. Combines the verbatim-vs-canonical reserved
//! model with forward padding — a third example for each of `reserved` and `pad`/`align`.
//!
//! Reserved-bearing types are builder/decode-constructed (the hidden `encode_mode` field).
//!
//! Run with: `cargo run -p bitsandbytes --example register`

use bnb::prelude::*; // the `1.bytes()` amount helper
use bnb::{bin, u3, u4, u5};

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Register {
    mode: u3,
    #[reserved]
    rsv: u5, // must-be-zero; fills byte 0 (3 + 5)
    #[br(pad_before = 1.bytes())] // a reserved gap byte before the value word
    value: u16,
    cmd: u4,
    #[reserved_with(u4::new(0xF))]
    guard: u4, // must-be-one; fills the last byte (4 + 4)
}

fn main() {
    // From the builder, the reserved fields default to their spec values — so it's canonical.
    let r = Register::builder()
        .mode(u3::new(0b101))
        .value(0x1234)
        .cmd(u4::new(0xA))
        .build()
        .unwrap();
    println!("{r:#?}");
    assert!(r.is_canonical());
    // mode(101) rsv(00000) | pad | value | cmd(1010) guard(1111)
    assert_eq!(r.to_bytes().unwrap(), [0xA0, 0x00, 0x12, 0x34, 0xAF]);

    // A peer left non-spec reserved bits (rsv all-ones, guard all-zeros).
    let wire = [0xBF, 0x00, 0x12, 0x34, 0xA0];
    let received = Register::decode_exact(&wire).unwrap();
    println!("received (verbatim): {received:#?}");

    // Verbatim re-emits the actual bits; the pad byte stays zero (it's positioning, not stored).
    assert_eq!(received.to_bytes().unwrap(), wire);
    assert!(!received.is_canonical());
    println!("canonical_diff = {:?}", received.canonical_diff());
    assert_eq!(received.canonical_diff(), vec!["rsv", "guard"]);

    // Canonical normalizes BOTH reserved fields back to spec.
    assert_eq!(
        received.to_canonical_bytes().unwrap(),
        [0xA0, 0x00, 0x12, 0x34, 0xAF]
    );

    println!("all checks passed");
}
