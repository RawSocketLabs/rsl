//! **flags** — `#[bitflags]` in depth: a capability flag set with set algebra, per-flag
//! accessors, iteration, **dual-use** retain-vs-truncate of unknown bits, and nesting in a
//! `#[bin]` message. (The flags analog of the `enums` example.)
//!
//! Run with: `cargo run -p bitsandbytes --example flags`

use bnb::{bin, bitflags};

/// Session capabilities — five flags in a `u8`, so bits 5..8 are reserved (currently unused),
/// which is what lets us demonstrate forward-compatible *retain* below.
#[bitflags(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct Caps {
    compress: bool,
    encrypt: bool,
    resume: bool,
    auth: bool,
    keepalive: bool,
}

impl Caps {
    /// A named combination, folded from the generated consts at compile time.
    const SECURE: Self = Self::ENCRYPT.union(Self::AUTH);
}

/// A `#[bin]` handshake embedding the flag set — it nests because `#[bitflags]` impls `Bits`.
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Handshake {
    version: u8,
    caps: Caps,
    session: u16,
}

fn main() {
    // --- set algebra ---
    let caps = Caps::COMPRESS | Caps::ENCRYPT | Caps::AUTH;
    println!("caps = {caps:?}  (bits {:08b})", caps.bits());
    assert!(caps.contains(Caps::SECURE)); // ENCRYPT | AUTH
    assert!(!caps.contains(Caps::RESUME));
    assert_eq!(caps - Caps::COMPRESS, Caps::SECURE);

    // --- per-flag accessors + the immutable `with_*` mutator ---
    let caps = caps.with_resume(true);
    assert!(caps.resume());

    // --- iterate the set flags ---
    print!("set flags:");
    for f in caps.iter() {
        print!(" {f:?}");
    }
    println!();

    // --- nest in a message + round-trip ---
    let hs = Handshake {
        version: 1,
        caps,
        session: 0x1234,
    };
    let bytes = hs.to_bytes().unwrap();
    println!("encoded handshake: {bytes:02x?}");
    assert_eq!(Handshake::decode_exact(&bytes).unwrap(), hs);
    println!("{hs:#?}");

    // --- dual-use: retain vs truncate unknown (reserved) bits ---
    // A newer peer set a reserved bit (0b1000_0000) this build doesn't define yet.
    let wire = 0b1000_0001u8; // COMPRESS + an unknown high bit
    let retained = Caps::from_bits(wire); // forward-compatible: keeps the unknown bit
    let truncated = Caps::from_bits_truncate(wire); // strict: drops it
    assert_eq!(retained.bits(), 0b1000_0001);
    assert_eq!(truncated.bits(), 0b0000_0001);
    println!(
        "unknown bit -> retain={:08b}  truncate={:08b}",
        retained.bits(),
        truncated.bits()
    );

    println!("all checks passed");
}
