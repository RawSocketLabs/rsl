/*!
`bits` — ergonomic, fast bit/byte field types for binary protocol codecs, with
first-class [`binrw`] integration.

> **Direction → `bnb`.** An owned, bit-aware successor that drops the external
> `binrw` dependency (and renames `bits` → `bnb`) is planned. Its target API is
> documented in the `design_preview` module (build with `--features doc-preview`)
> and in `ROADMAP.md`, and is **not yet implemented**. Everything below describes
> the crate as it ships **today**.

It provides three things, designed to *compose*:

- **Arbitrary-width integers** ([`u1`]..[`u127`], via [`UInt`]) for sub-byte
  fields — a dependency-free replacement for `arbitrary-int`.
- **`#[bitfield]`** — an attribute macro that packs typed fields into a single
  backing integer with **explicit, independent control of bit order**
  (MSB/LSB-first) **and byte order** (big/little). It generates getters,
  immutable `with_*` setters, raw access, allocation-free `*_bytes` conversions,
  and — with the default `binrw` feature — `BinRead`/`BinWrite` impls so the type
  drops into a `#[binrw]` struct with **no `map` glue**.
- **`#[derive(BitEnum)]`** — enum ⇄ integer at a chosen width, with an optional
  `#[catch_all]` variant that preserves unknown values (the dual-use convention).

The aim is to retire the workspace's stack of overlapping helpers
(`modular-bitfield`(`-msb`), `bitfield-struct`, `bitbybit`, `arbitrary-int`,
`num_enum`) behind one fast, integer-backed (shift/mask, no `bitvec`) crate.

# Example — a DNS-style 16-bit header field

```
use bits::{bitfield, BitEnum, u4, u5, u7};

# #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
# struct Flags(bits::u7);
# impl bits::Bits for Flags {
#     const BITS: u32 = 7;
#     fn into_bits(self) -> u128 { self.0.into_bits() }
#     fn from_bits(raw: u128) -> Self { Flags(u7::from_bits(raw)) }
# }
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum RCode {
    NoError,   // 0
    FormErr,   // 1
    ServFail,  // 2
    #[catch_all]
    Other(u4), // any other 4-bit value
}

// MSB-first packing (network/RFC order), big-endian on the wire.
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct State {
    opcode: u5,   // first field -> high bits
    flags:  Flags,
    rcode:  RCode, // last field -> low bits
}

let s = State::new()
    .with_opcode(u5::new(2))
    .with_rcode(RCode::ServFail);
assert_eq!(s.rcode(), RCode::ServFail);
// opcode occupies bits 11..=15 (the high 5 of the u16).
assert_eq!(s.to_be_bytes()[0] >> 3, 2);
```

# Bit order vs. byte order

These are independent knobs, which is the whole point:

- `bits = msb | lsb` — does the **first** declared field land in the high or low
  bits of the backing integer. Default: `msb` (matches RFC ASCII-art layouts).
- `bytes = be | le` — endianness of the backing integer when serialized.
  Default: `be`.

# binrw integration (the `binrw` feature, on by default)

With the feature on, `#[bitfield]` and `#[derive(BitEnum)]` types implement
[`binrw::BinRead`]/[`binrw::BinWrite`] using their declared byte order, so:

```ignore
#[binrw]
#[brw(big)]
struct Header {
    id: u16,
    state: State,   // no #[br(map = ...)] / #[bw(map = ...)] needed
    qd: u16,
}
```

Turn the feature off (`default-features = false`) for a standalone,
dependency-light bit/byte library — the path to dropping `binrw` entirely.

[`binrw`]: https://docs.rs/binrw
[`binrw::BinRead`]: https://docs.rs/binrw/latest/binrw/trait.BinRead.html
[`binrw::BinWrite`]: https://docs.rs/binrw/latest/binrw/trait.BinWrite.html
*/

// Every public item must be documented (the `uN` aliases are the one self-
// evident exception, allowed at their module).
#![deny(missing_docs)]

pub mod bitstream;
pub mod builder;
/// Target-design rustdoc preview for the future `bnb` codec — doc-only, **not
/// yet implemented**. Enabled by the `doc-preview` feature for review; see
/// `ROADMAP.md`. Renders like binrw's `docs::attribute` reference.
#[cfg(feature = "doc-preview")]
pub mod design_preview;
pub mod error;
mod field;
pub mod int;

pub use bitstream::{BitDecode, BitEncode, BitError, BitReader, BitWriter, StreamBitReader};
pub use builder::BuilderError;
pub use error::{Error, Result, UnknownDiscriminant};
pub use field::{BitOrder, Bitfield, Bits, ByteOrder};
pub use int::{UInt, *};

// Re-export the macros so users depend only on `bits`. A derive macro and a
// trait may share a name (like `Debug`) — they live in different namespaces —
// so `BitEnum`/`BitDecode`/`BitEncode` are each both a derive and a trait.
pub use bits_macros::{BitDecode, BitEncode, BitEnum, BitsBuilder, bitfield, bitflags};

// `#[wire]`/`#[bitwire]` wrap binrw, so they exist only with the `binrw` feature.
#[cfg(feature = "binrw")]
pub use bits_macros::{bitwire, wire};

/// Marker trait implemented by `#[derive(BitEnum)]` enums: a [`Bits`] value
/// whose representation is an integer discriminant of a fixed width.
pub trait BitEnum: Bits {}

/// Implementation details referenced by macro-generated code. Not a stable API.
#[doc(hidden)]
pub mod __private {
    pub use crate::bitstream::{BitDecode, BitEncode, BitError, BitReader, BitWriter};
    #[cfg(feature = "binrw")]
    pub use crate::bitstream::{read_bits_region, write_bits_region};
    pub use crate::error::UnknownDiscriminant;
    pub use crate::field::{BitOrder, Bitfield, Bits, ByteOrder};

    /// Re-export of `binrw` so generated `BinRead`/`BinWrite` impls can name it
    /// without the user crate depending on `binrw` directly.
    #[cfg(feature = "binrw")]
    pub use ::binrw;
}
