//! **arbitrary_width** — arbitrary bit widths and a *wide* enum in one sub-byte message.
//!
//! `SyncPat` is a 48-bit `#[derive(BitEnum)]`: it tracks a long sync/magic word that is far too
//! wide for an ordinary small enum. It sits between two `u3` fields, so the whole `Frame` is
//! `3 + 48 + 3 = 54` bits — not byte-aligned, which is exactly `#[bin]`'s wheelhouse.
//!
//! A recognized word decodes to its named variant (`T1`); any other 48-bit value is preserved by
//! the `#[catch_all]` as `Custom(u48)` — the dual-use pattern, so an unknown sync round-trips
//! instead of being rejected.
//!
//! Run with: `cargo run -p bitsandbytes --example arbitrary_width`

use bnb::{BitEnum, bin, u3, u48};

/// A 48-bit sync pattern: `T1` is the one we recognize, `Custom` retains any other value.
#[derive(BitEnum, Copy, Clone, Debug, PartialEq, Eq)]
#[bit_enum(u48)]
#[repr(u64)] // a catch_all (tuple) variant alongside an explicit discriminant needs an explicit repr
enum SyncPat {
    T1 = 0xF234_5678_9ABC,
    #[catch_all]
    Custom(u48),
}

/// `lead` (3 bits) │ `sync` (48 bits) │ `trail` (3 bits) = 54 bits, big-endian.
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    lead: u3,
    sync: SyncPat,
    trail: u3,
}

fn main() {
    // 1. A frame carrying the recognized sync word round-trips through its named variant.
    let known = Frame {
        lead: u3::new(0b111),
        sync: SyncPat::T1,
        trail: u3::new(0b101),
    };
    let bytes = known.to_bytes().unwrap();
    // 54 bits packs into 7 bytes (the final 2 bits are padding); decode_exact tolerates them.
    println!("known sync : {} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Frame::decode_exact(&bytes).unwrap(), known);
    assert_eq!(known.sync, SyncPat::T1);

    // 2. An unrecognized 48-bit sync is preserved by `#[catch_all]`, not rejected (dual-use).
    let custom = Frame {
        lead: u3::new(0),
        sync: SyncPat::Custom(u48::new(0x0102_0304_0506)),
        trail: u3::new(0),
    };
    let bytes = custom.to_bytes().unwrap();
    println!("custom sync: {} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Frame::decode_exact(&bytes).unwrap(), custom);

    println!("{known:#?}");
    println!("all checks passed");
}
