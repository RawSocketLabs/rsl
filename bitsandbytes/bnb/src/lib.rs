/*!
`bnb` — an owned, bit-aware binary codec: ergonomic, fast bit/byte field types and
the unified `#[bin]` whole-message codec. No `binrw` dependency.

It provides, designed to *compose*:

- **Arbitrary-width integers** ([`u1`]..[`u127`], via [`UInt`]) for sub-byte
  fields — a dependency-free replacement for `arbitrary-int`.
- **`#[bitfield]`** — an attribute macro that packs typed fields into a single
  backing integer with **explicit, independent control of bit order**
  (MSB/LSB-first) **and byte order** (big/little). It generates getters,
  immutable `with_*` setters, raw access, and allocation-free `*_bytes` conversions.
- **`#[derive(BitEnum)]`** — enum ⇄ integer at a chosen width, with an optional
  `#[catch_all]` variant that preserves unknown values (the dual-use convention).
- **`#[bin]`** — the unified whole-message codec (see below): reads/writes a struct
  at arbitrary bit offsets, with the full directive surface that subsumed our
  former `binrw` usage.

The aim is to retire the workspace's stack of overlapping helpers
(`modular-bitfield`(`-msb`), `bitfield-struct`, `bitbybit`, `arbitrary-int`,
`num_enum`, and our use of `binrw`) behind one fast, integer-backed
(shift/mask, no `bitvec`) crate.

# Example — a DNS-style 16-bit header field

```
use bnb::{bitfield, BitEnum, u4, u5, u7};

# #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
# struct Flags(bnb::u7);
# impl bnb::Bits for Flags {
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

# The `#[bin]` codec

Whole-message bit-aware codec: `#[bin]` (magic/count/ctx/map/if/calc·temp/reserved/
positioning/validate) over a `Source`/`SeekSource`/`BufSource`/`SeekReader` I/O
ladder, with an opt-in `bytes` feature for async framing. It is the owned successor
to our former binrw usage — no binrw dependency.
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

pub use bitstream::{
    BitAmount, BitDecode, BitEncode, BitError, BitReader, BitWriter, BufSource, DecodeWith,
    EncodeWith, ErrorKind, FixedBitLen, Layout, SeekReader, SeekSource, Sink, Source,
    StreamBitReader,
};

/// Zero-copy `bytes`-crate adapters (the `bytes` feature).
#[cfg(feature = "bytes")]
pub use bitstream::{BytesReader, BytesWriter};

/// Common imports for the codec — the typed positioning amounts (`4.bits()`,
/// `3.bytes()`) used by `#[br(pad_before = …)]` etc.
pub mod prelude {
    pub use crate::BitAmount;
}
pub use builder::BuilderError;
pub use error::{Error, Result, UnknownDiscriminant};
pub use field::{BitOrder, Bitfield, Bits, ByteOrder};
pub use int::{UInt, *};

// Re-export the macros so users depend only on `bnb`. A derive macro and a
// trait may share a name (like `Debug`) — they live in different namespaces —
// so `BitEnum`/`BitDecode`/`BitEncode` are each both a derive and a trait.
pub use bnb_macros::{BitDecode, BitEncode, BitEnum, BitsBuilder, bin, bitfield, bitflags};

/// Marker trait implemented by `#[derive(BitEnum)]` enums: a [`Bits`] value
/// whose representation is an integer discriminant of a fixed width.
pub trait BitEnum: Bits {}

/// Implementation details referenced by macro-generated code. Not a stable API.
#[doc(hidden)]
pub mod __private {
    pub use crate::bitstream::{
        BitDecode, BitEncode, BitError, BitReader, BitWriter, FixedBitLen, Layout, SeekSource,
        Sink, Source, align_read, align_write, bits_of, decode_consume, decode_exact,
        decode_exact_with, decode_peek, encode_to_vec, encode_to_vec_with, encode_to_writer,
        read_byte_array, read_mapped, read_try_mapped, skip_read, skip_write, verify_magic,
        write_byte_array, write_mapped,
    };
    pub use crate::error::UnknownDiscriminant;
    pub use crate::field::{BitOrder, Bitfield, Bits, ByteOrder};
}
