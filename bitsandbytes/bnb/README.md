# bnb

[![crates.io](https://img.shields.io/crates/v/bitsandbytes.svg)](https://crates.io/crates/bitsandbytes)
[![docs.rs](https://docs.rs/bitsandbytes/badge.svg)](https://docs.rs/bitsandbytes)

An **owned, bit-aware binary codec**: ergonomic, fast bit/byte field types plus a
unified `#[bin]` whole-message macro for binary protocols. No external codec
dependency — `bnb` is self-contained.

Published on crates.io as **`bitsandbytes`**; import it as `bnb`:
`bnb = { package = "bitsandbytes", version = "0.1" }`. Docs: <https://docs.rs/bitsandbytes>.

`bnb` collapses a stack of overlapping helpers — `modular-bitfield(-msb)`,
`bitfield-struct`, `bitbybit`, `arbitrary-int`, `num_enum`, and a `binrw`-style
codec — into one crate that is:

- **Integer-backed and fast** — bitfields are plain shift/mask on a single backing
  integer (no `bitvec`); the stream codec reads/writes at arbitrary **bit** offsets.
- **Explicit about ordering** — independent control of **bit order** (MSB/LSB-first)
  and **byte order** (big/little), exactly what protocol layouts need.
- **Dual-use by default** — the guided path is RFC-correct, but parsers stay
  permissive (unknown enum values are preserved as a catch-all, never rejected), so
  fuzzing / red-teaming / interop testing can emit deliberately non-conformant data.

## What's in it

| Item | Replaces | Purpose |
|---|---|---|
| `u1`..`u127` (`UInt<T, N>`) | `arbitrary-int` | sub-byte unsigned integers |
| `#[bitfield]` | `modular-bitfield(-msb)`, `bitbybit`, `bitfield-struct` | pack typed fields into one integer |
| `#[derive(BitEnum)]` | `num_enum`, `bitbybit::bitenum` | enum ⇄ integer with an optional catch-all |
| `#[bitflags]` | `bitflags` | named single-bit flag sets with set algebra |
| `#[derive(BitsBuilder)]` | `derive_builder` (for bit/byte structs) | required-by-default builder |
| **`#[bin]`** | a hand-written codec | **whole-message codec**: magic/count/ctx/map/if/calc/reserved/positioning/validate |

## Bitfields, enums, flags

```rust
use bnb::{bitfield, BitEnum, u4};

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum RCode {
    NoError,
    FormErr,
    ServFail,
    #[catch_all]
    Other(u4), // unknown values preserved (dual-use)
}

// MSB-first packing (network order), big-endian on the wire.
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct State {
    opcode: u4,
    flags:  u8,
    rcode:  RCode,
}

let s = State::new()
    .with_opcode(u4::new(2))
    .with_rcode(RCode::ServFail);

assert_eq!(s.to_be_bytes(), [0x20, 0x02]);
assert_eq!(s.rcode(), RCode::ServFail);
```

A byte-aligned `#[derive(BitEnum)]` also gets `num_enum`-parity conversions
(`From<Enum> for uN` always; `From<uN>`/`TryFrom<uN>` depending on the catch-all),
so a magic-byte enum needs no hand-written `From` impl or round-trip test.

`#[bitflags]` packs named single-bit flags into one integer with full set algebra;
a flag set implements `Bits`, so it nests in a `#[bitfield]` and in a `#[bin]`
message:

```rust
use bnb::bitflags;

#[bitflags(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TcpFlags { fin: bool, syn: bool, rst: bool, psh: bool, ack: bool, urg: bool, ece: bool, cwr: bool }

let f = TcpFlags::SYN | TcpFlags::ACK;
assert!(f.contains(TcpFlags::SYN));
assert!(f.ack()); // per-flag accessor
```

## The `#[bin]` whole-message codec

`#[bin]` folds the read/write codec and a required-by-default builder over one
struct. It reads/writes fields at arbitrary bit offsets, so the **same** attribute
handles byte-aligned headers and sub-byte frames. A field that is another
`#[bitfield]`/`BitEnum`/`#[bitflags]` nests with no glue.

```rust,ignore
use bnb::{bin, bitfield, u3, u4, BitEnum};

#[bin(big, validate = header_soundness)]
#[derive(Debug, Clone, PartialEq)]
struct Header {
    id: u16,
    flags: Flags,        // a 16-bit #[bitfield], nested as a leaf
    qdcount: u16,
    ancount: u16,
    nscount: u16,
    arcount: u16,
}

// Derived & framed: `len` is never stored — it is read into a temp that drives the
// Vec, and recomputed from the data on write, so it can't drift.
#[bin(big, magic = 0xCAFEu16)]
#[derive(Debug, Clone, PartialEq)]
struct Frame {
    #[br(temp)]
    #[bw(calc = self.payload.len() as u8)]
    len: u8,
    #[br(count = len)]
    payload: Vec<u8>,
}

let header = Header::builder().id(0x1234).flags(flags)
    .qdcount(1).ancount(1).nscount(0).arcount(0).build()?; // Err names any unset field
let bytes = header.to_bytes()?;            // -> 12 bytes
let parsed = Header::decode_exact(&bytes)?; // exact inverse
```

The generated API per `#[bin]` type: `decode` (over a `Source` cursor) / `decode_all` /
`decode_iter` / `peek` / `decode_exact` to read; `to_bytes` / `encode` (to any `io::Write`) /
`BitEncode::bit_encode` (to a `Sink`) to write; and `Type::builder()` / `Type::new(…)`. See
[`examples/bin_message.rs`](examples/bin_message.rs) for a complete, runnable DNS-header +
framed-payload round-trip.

### Directives & I/O

- **Struct-level:** `big`/`little`, `bit_order = msb|lsb`, `read_only`/`write_only`,
  `no_builder`, `forward_only`, `magic = <expr>`, `ctx(name: Ty, …)`,
  `validate = <path>`.
- **Field-level** (`#[br]`/`#[bw]`): `count`, `ctx { … }`, `temp` + `calc`, `if(…)`,
  `map`/`try_map` (+ inverse `bw(map)`), `parse_with`/`write_with`, `ignore`,
  `pad_before/after` / `align_before/after` / `restore_position`,
  `#[reserved]` / `#[reserved_with(…)]`, and `#[try_str]` (a `Debug`-rendering hint —
  a byte buffer prints as a string when valid UTF-8, else hex).
- **I/O ladder:** decode from a `&[u8]` slice (`BitReader`), a forward `Read`
  (`StreamBitReader`), a bounded retain-and-seek socket adapter (`BufSource`), a pushable
  in-memory buffer (`BitBuf`), or a `Read + Seek` file (`SeekReader`); with the opt-in
  **`bytes`** feature, the zero-copy `BytesReader`/`BytesWriter`.

`validate` gates `build()` only — the **parser stays permissive** (it never rejects
representable input, per the dual-use rule), so deliberately malformed messages are
still decodable.

## Bit order vs. byte order

- `bits = msb | lsb` (default `msb`): does the **first** declared field land in the
  high or low bits of the backing integer.
- `bytes = be | le` (default `be`): byte order of the backing integer on the wire.

These are independent knobs — the whole point. MSB-first big-endian matches the
ASCII-art layouts in RFCs.

## Field widths

In order of precedence: an explicit `#[bits(N)]`; an explicit `#[bits(A..=B)]`
range (which fixes the absolute offset — the manual layout escape hatch); or, by
default, `<FieldType as bnb::Bits>::BITS`. Use inference/widths for automatic
layout, or ranges on every field for fully manual layout — the two styles cannot be
mixed in one struct.

## Crate layout

`bnb` (this crate, the runtime types) re-exports the macros from `bnb-macros` (the
proc-macro crate). Depend only on `bnb`. The optional `bytes` feature adds the
`bytes`-crate I/O adapters; the core is otherwise dependency-light.

See [`DESIGN.md`](DESIGN.md) for the design rationale and [`ROADMAP.md`](ROADMAP.md)
for the capability/status summary.

## Inspiration

`bnb` collapses the capabilities of several excellent crates into one. The
arbitrary-width integers echo [`arbitrary-int`]; the bitfield packing echoes
[`modular-bitfield`], [`bitfield-struct`], and [`bitbybit`]; the enum ⇄ integer
mapping echoes [`num_enum`]; and — most of all — the declarative, bidirectional
codec and its `#[br]`/`#[bw]` attribute vocabulary are modeled on
[`binrw`](https://github.com/jam1garner/binrw), so the two feel like one toolkit.

`bnb` shares no code with any of them — it is a from-scratch implementation,
extended to do the one thing a byte-oriented `Read + Seek` codec cannot: read and
write fields at arbitrary **bit** offsets. See [`ACKNOWLEDGMENTS.md`](ACKNOWLEDGMENTS.md)
for the full credit.

[`arbitrary-int`]: https://crates.io/crates/arbitrary-int
[`modular-bitfield`]: https://crates.io/crates/modular-bitfield
[`bitfield-struct`]: https://crates.io/crates/bitfield-struct
[`bitbybit`]: https://crates.io/crates/bitbybit
[`num_enum`]: https://crates.io/crates/num_enum
