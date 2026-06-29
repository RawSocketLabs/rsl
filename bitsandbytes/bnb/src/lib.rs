/*!
`bnb` — an owned, bit-aware binary codec: ergonomic, fast bit/byte field types and
the unified `#[bin]` whole-message codec.

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
  at arbitrary bit offsets, with a rich, `binrw`-inspired directive surface.

The aim is to collapse a whole stack of overlapping helpers —
`modular-bitfield`(`-msb`), `bitfield-struct`, `bitbybit`, `arbitrary-int`,
`num_enum`, and a `binrw`-style codec — into one fast, integer-backed
(shift/mask, no `bitvec`) crate. See [Inspiration](#inspiration).

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
ladder, with an opt-in `bytes` feature for async framing.

# Feature flags & `no_std`

`bnb` is `no_std`-compatible — it always needs `alloc` (the codec produces
`Vec<u8>` and owns variable-length payloads), but not `std`. Build with
`default-features = false` for an embedded target.

- **`std`** *(default)* — the `std::io` ladder ([`StreamBitReader`], [`BufSource`],
  [`SeekReader`], [`Source::as_read`]/[`Sink::as_write`]), the `From<std::io::Error>`
  bridge, and the `encode(writer)` convenience ([`EncodeExt`]). The `#[br(dbg)]`
  directive (which emits a `tracing` event) is also `std`-only.
- **`bytes`** — the zero-copy `bytes`-crate adapters; implies `std` (async/tokio framing).
- **`tokio`** — [`BinCodec`], a `tokio_util::codec` `Decoder`/`Encoder` for any `#[bin]`
  message: `Framed::new(tcp, BinCodec::<T>::new())` (a stream) or `UdpFramed::new(udp, …)` (a
  datagram `Stream + Sink` of `(T, addr)`) — one codec, both async transports. Implies `bytes`.
- **`net`** — ergonomic `std` socket helpers: [`MessageStream`] (whole-message read/write over
  any `Read + Write`, e.g. a `TcpStream`, no `try_clone`) and [`MessageDatagram`] (`send_message`/
  `recv_message` over a sealed [`DatagramSocket`] — `UdpSocket` or `UnixDatagram`). Implies `std`.
- **`mock`** — test-only in-memory transports for exercising `net` code without a real socket:
  [`MockDatagramSocket`] (a [`DatagramSocket`]) and [`MockStream`] (a `Read + Write`, with chunked
  delivery to drive the read-more path). Put it in your `[dev-dependencies]`. Implies `net`.

Without `std` you still get the full macro surface plus: decode from a `&[u8]`
([`BitReader`], `Type::decode`/`decode_all`/`decode_iter`/`decode_exact`/`peek`) and encode to a
`Vec<u8>` (`Type::to_bytes`/`to_canonical_bytes`, or [`BitEncode::bit_encode`] over a [`Sink`]). You lose
only the streaming `std::io` adapters and `encode(&mut impl Write)`; on `no_std`,
encode with `to_bytes()` and write the bytes to your transport yourself.

# Inspiration

`bnb` stands on the shoulders of several excellent crates, collapsing their
capabilities into one: the arbitrary-width integers of `arbitrary-int`; the
bitfield packing of `modular-bitfield`, `bitfield-struct`, and `bitbybit`; the
enum ⇄ integer mapping of `num_enum`; and — most of all — the declarative,
bidirectional codec design of [`binrw`](https://github.com/jam1garner/binrw),
whose `#[br]`/`#[bw]` attribute vocabulary `#[bin]` deliberately echoes so the two
feel like one toolkit. `bnb` shares no code with these crates; it is a from-scratch
implementation, extended to do the one thing a byte-oriented `Read + Seek` codec
cannot: read and write fields at arbitrary **bit** offsets. See `ACKNOWLEDGMENTS.md`
for the full credit.

# Guide

The [`guide`] module is a set of worked, runnable walkthroughs — start there for a
tour of the crate and the rationale behind each piece. Reading order:

1. [`guide::quick_start`] — a five-minute tour of every macro.
2. [`guide::numbers`] — the arbitrary-width integers (`u1`..`u127`) and the [`Bits`]
   trait that lets everything compose.
3. [`guide::bitfields`] — `#[bitfield]`: bit order, byte order, widths and ranges,
   nesting.
4. [`guide::enums`] — `#[derive(BitEnum)]`: catch-all vs. closed, `num_enum` parity.
5. [`guide::flags`] — `#[bitflags]`: single-bit flag sets with set algebra.
6. [`guide::builders`] — `#[derive(BitsBuilder)]`: the required-by-default builder.
7. [`guide::bin_codec`] — `#[bin]`: a whole protocol header, end to end.
8. [`guide::directives`] — the field-directive reference, one example each.
9. [`guide::mapping`] — `#[bin(map/bw_map = …)]`: a whole struct mapped to/from a wire type.
10. [`guide::dispatch`] — `#[bin]` on an enum: tagged-union dispatch by wire `magic` or off-wire `tag`.
11. [`guide::io`] — the `Source`/`Sink` I/O ladder (slice, stream, socket, file, `bytes`).
12. [`guide::errors`] — position-aware errors and the streaming `Incomplete` signal.
13. [`guide::dual_use`] — the compliant-by-default-but-violatable philosophy.
14. [`guide::composition`] — how the pieces nest and size each other.
*/

// Every public item must be documented (the `uN` aliases are the one self-
// evident exception, allowed at their module).
#![deny(missing_docs)]
// On docs.rs (which sets `--cfg docsrs` and builds on nightly), annotate every
// feature-gated item with an "Available on crate feature …" badge. A no-op on
// stable, where `docsrs` is never set.
#![cfg_attr(docsrs, feature(doc_cfg))]
// `bnb` is `no_std` when built without the (default) `std` feature; `alloc` is
// always required — the codec produces `Vec<u8>` and owns variable-length
// payloads/error messages. The `std` feature re-enables the `std::io` ladder
// (`StreamBitReader`/`BufSource`/`SeekReader`, `encode(writer)`).
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
// Macro-generated code references the runtime by the path `proc-macro-crate`
// resolves — `::bnb` for the crate itself. This self-alias makes that path
// resolve inside the crate's own modules (the lib name is `bnb`).
extern crate self as bnb;

pub mod bitstream;
pub mod builder;
/// Async framing — a [`tokio_util::codec`] adapter (the `tokio` feature).
#[cfg(feature = "tokio")]
pub mod codec;
pub mod error;
mod field;
pub mod guide;
pub mod int;
/// Ergonomic `std` socket helpers — [`MessageStream`] + [`MessageDatagram`] (the `net` feature).
#[cfg(feature = "net")]
pub mod net;

pub use bitstream::{
    BitAmount, BitBuf, BitDecode, BitEncode, BitError, BitReader, BitWriter, DecodeWith,
    EncodeMode, EncodeWith, ErrorKind, FixedBitLen, Layout, SeekSource, Sink, Source,
};

/// The `std::io` I/O ladder and writer conveniences — only with the (default)
/// `std` feature. Without it, `bnb` is `no_std + alloc`: decode from a `&[u8]`
/// (`BitReader`), encode to a `Vec<u8>` (`to_bytes`/`to_canonical_bytes`).
#[cfg(feature = "std")]
pub use bitstream::{BufSource, EncodeExt, SeekReader, SinkWriter, SourceReader, StreamBitReader};

/// Zero-copy `bytes`-crate adapters (the `bytes` feature).
#[cfg(feature = "bytes")]
pub use bitstream::{BytesReader, BytesWriter};

/// The async `tokio_util` codec adapter (the `tokio` feature).
#[cfg(feature = "tokio")]
pub use codec::BinCodec;

/// Ergonomic `std` socket helpers (the `net` feature).
#[cfg(feature = "net")]
pub use net::{DatagramSocket, MessageDatagram, MessageStream};
#[cfg(feature = "mock")]
pub use net::{MockDatagramSocket, MockStream};

/// Common imports for the codec — the typed positioning amounts (`4.bits()`,
/// `3.bytes()`) used by `#[br(pad_before = …)]` etc., plus the [`EncodeExt`] trait that
/// carries `encode(writer)` (the `std` feature).
pub mod prelude {
    pub use crate::BitAmount;
    #[cfg(feature = "std")]
    pub use crate::EncodeExt;
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
    /// The `std::io::Write`-based encode helpers, behind the `std` feature.
    #[cfg(feature = "std")]
    pub use crate::bitstream::encode_to_writer_with;
    pub use crate::bitstream::{
        BitDecode, BitEncode, BitError, BitReader, BitWriter, FixedBitLen, Layout, SeekSource,
        Sink, Source, align_read, align_write, bits_of, decode_all, decode_exact,
        decode_exact_with, decode_iter, decode_mapped_msg, decode_peek, decode_peek_with,
        decode_try_mapped_msg, encode_mapped_msg, encode_to_vec, encode_to_vec_with, peek_bytes,
        read_byte_array, read_mapped, read_try_mapped, skip_read, skip_write, verify_magic,
        write_byte_array, write_mapped,
    };
    pub use crate::error::UnknownDiscriminant;
    pub use crate::field::{BitOrder, Bitfield, Bits, ByteOrder};
    /// Owned-collection re-exports so macro-generated code names neither `std` nor
    /// `alloc` directly (the user crate need not declare `extern crate alloc`).
    pub use ::alloc::{string::String, vec, vec::Vec};
    /// Re-exported for the `#[br(dbg)]` directive's generated `trace!` call, so a user
    /// of the directive needs no direct `tracing` dependency. `#[br(dbg)]` is a
    /// `std`-only debugging aid (see the `tracing` dependency note in `Cargo.toml`).
    #[cfg(feature = "std")]
    pub use ::tracing;

    /// Adaptive byte-buffer formatter for the `#[bin]` `#[try_str]` field hint: renders a
    /// buffer that is **valid UTF-8** as a quoted, escaped string (`"hello"`, `"a\nb"`) and any
    /// buffer with a non-UTF-8 byte as **hex bytes** (`[de, ad, be, ef]`). All-or-nothing — it
    /// never replaces bytes (no lossy `�`), so the rendering can't misrepresent the wire.
    pub struct TryStr<'a, T: ?Sized>(pub &'a T);

    impl<T: AsRef<[u8]> + ?Sized> ::core::fmt::Debug for TryStr<'_, T> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            let bytes = self.0.as_ref();
            match ::core::str::from_utf8(bytes) {
                ::core::result::Result::Ok(s) => ::core::fmt::Debug::fmt(s, f),
                ::core::result::Result::Err(_) => write!(f, "{bytes:02x?}"),
            }
        }
    }
}
