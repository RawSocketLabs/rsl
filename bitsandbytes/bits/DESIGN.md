# `bits` — design proposal for a unified bit/byte protocol-codec utility

Status: **proposal / RFC** — 2026-06-11. Audience: workspace maintainers.

## 1. Executive summary

This workspace currently leans on **six** external bit/int/enum helper crates
(`modular-bitfield`, `modular-bitfield-msb`, `bitfield-struct`, `bitbybit`,
`arbitrary-int`, `num_enum`) plus three byte-domain helpers (`bytes`,
`crc32fast`, `macaddr`), all bridged to **`binrw`** by hand. The bitfield crates
overlap almost completely, are chosen ad hoc (usually to get the right *bit
order*), and each bridges to `binrw` differently. `application/nbt` alone pulls
in three of them (one of which, `bitfield-struct`, is a **dead dependency**).

**Recommendation (phased, low-regret):**

1. **Build `bits` as an integer-backed bit/int/enum layer that natively
   implements `binrw`'s traits.** This collapses the six bit/int/enum crates
   into one and *deletes the `#[br(map=…)]`/`#[bw(map=…)]` glue entirely*, while
   keeping `binrw` as the byte-stream codec. Fast (shift/mask, no `bitvec`),
   ergonomic, and validated against DNS/NBT/SMB. **This is where the actual pain
   is — fix it first.**
2. **Put a thin codec seam (`bits::codec`) under the bitfields**, with a `binrw`
   bridge, so a future in-house stream codec is a *drop-in swap* rather than a
   rewrite. This makes "should we replace `binrw`?" a **deferrable** decision.
3. **Do not build a `binrw` replacement first.** `binrw`'s value is its mature
   stream machinery (Read+Seek, args, magic, conditionals, `until_eof`, error
   spans) — none of which is your pain point. Revisit a full codec only if a
   concrete limitation bites after step 1; by then the seam makes it mechanical.

Net effect of step 1: drop `arbitrary-int`, `num_enum`, `modular-bitfield`,
`modular-bitfield-msb`, `bitfield-struct`, `bitbybit` → one `bits` crate, with a
path to also absorb `bytes`/`macaddr`/`crc32fast` and eventually `binrw` /
`derive_builder` later.

---

## 2. Audit — what's actually in use, and the redundancy

### 2.1 Inventory

| Crate | Role | Used by | Backing / order |
|---|---|---|---|
| `bitbybit` | bitfield derive | `dns` | integer-backed, bit **ranges**, order via bit index |
| `arbitrary-int` | `u1..u127` types | `dns` | companion to `bitbybit` |
| `modular-bitfield` | bitfield derive (**LSB**) | `smb`, `nbt` | byte-array-backed, bit **widths** |
| `modular-bitfield-msb` | bitfield derive (**MSB**) | `nbt` | byte-array-backed, bit widths |
| `bitfield-struct` | bitfield derive | `nbt` (**declared, never used**) | integer-backed |
| `num_enum` | enum ⇄ int (`catch_all`) | `arp`, `nbt` | — |
| `bytes` | byte buffers | `smb` | — |
| `crc32fast` | CRC32 | `ethernet` | — |
| `macaddr` | MAC address type | `arp` | — |

### 2.2 The smoking gun: one problem, solved three ways

DNS and NBT are near-identical protocols; both collapse `opcode/flags/rcode`
into a 16-bit field. They implement the *same* structure with *different* crates
and *different* `binrw` bridges:

**DNS — `bitbybit` (integer-backed, binrw-derivable, bit ranges):**
```rust
#[bitfield(u16, default = 0)]
#[derive(BinWrite, BinRead, Debug, PartialEq, Eq)]   // derives directly on the u16 backing
pub struct State {
    #[bits(11..=15, rw)] opcode: OpCode,   // explicit MSB-region range
    #[bits(4..=10, rw)]  flags:  Flags,
    #[bits(0..=3, rw)]   rcode:  RCode,
}
```

**NBT — `modular-bitfield-msb` (byte-array-backed, needs map glue, bit widths):**
```rust
#[bitfield]
#[derive(BinWrite, BinRead, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[br(map = Self::from_bytes)]            // <-- glue
#[bw(map = |&x| Self::into_bytes(x))]    // <-- glue
pub(crate) struct State {
    #[bits = 5] opcode: OpCode,
    #[bits = 7] flags:  Flags,
    #[bits = 4] rcode:  RValue,
}
```

**SMB — `modular-bitfield` (LSB), same glue, opposite byte order:**
```rust
#[bitfield]
#[binrw]
#[brw(little)]                           // SMB is little-endian; DNS/NBT are big
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecurityMode { user_mode: bool, /* … */ reserved: B4 }
```

### 2.3 Observations that drive the design

- **Bit order is the deciding factor.** `nbt` reaches for `modular-bitfield-msb`
  purely to get MSB-first packing (network/RFC diagram order). LSB-first crates
  put the fields in the wrong order. A unified tool **must** make bit order an
  explicit, per-type knob.
- **Byte order varies by protocol** (`be` for DNS/NBT, `little` for SMB) — also
  must be explicit and orthogonal to bit order.
- **Two bridging strategies, both warts.** Integer-backed types (`bitbybit`,
  `bitfield-struct`) can `derive(BinRead, BinWrite)` directly; byte-array types
  (`modular-bitfield*`) need `#[br(map)]`/`#[bw(map)]`. Neither is *native* —
  binrw has no idea bit fields exist.
- **The "collapsed-state + builder" dance recurs.** Both DNS and NBT headers use
  `#[builder(setter(skip))]` + `#[bw(calc = State::new().with_x(…).with_y(…))]`
  to assemble the private bitfield from individual `derive_builder` fields, plus
  `#[brw(ignore)]` builder-only fields (`check_soundness`). This is the
  `binrw`+`derive_builder`+bitfield triad, and it's clumsy.
- **Cruft & drift.** `bitfield-struct` is a dead dep in `nbt`; `nbt` also
  declares both `modular-bitfield` and `-msb`. `nbt` is on `binrw 0.13` /
  `derive_builder 0.12` while `socks`/`tftp`/`icmp` are on `binrw 0.15` /
  `derive_builder 0.20`.
- **The existing `bits` crate is broken scaffolding:** declared
  `proc-macro = true` yet exports runtime types (illegal together), macro code
  written for `syn 1.x` against a `syn 2.0` manifest, and it is not a workspace
  member (builds nothing).

---

## 3. Ecosystem survey

### 3.1 Byte/bit codecs (the `binrw` layer)

| Crate | Model | Bit fields | Byte order | Speed | Notes |
|---|---|---|---|---|---|
| **binrw** | derive, `Read+Seek`/`Write+Seek` stream | **none** (needs glue) | explicit `#[brw(be/le)]`, per-field | fast, byte-aligned | mature: args, magic, conditional, `until_eof`, error spans. What you use. |
| **deku** | derive, slice/`bitvec` | **native** (`bits`, `bit_order`) | explicit `endian` + `bit_order` | slower on bits (`bitvec` backing taxes even byte paths) | closest to "one crate does it all," but the `bitvec` cost is exactly what you want to avoid. |
| **nom** | parser **combinators** (fns) | via `nom::bits` | manual | very fast, zero-copy | parse-only (write separately, e.g. `cookie-factory`); not declarative. |
| **scroll** | `Pread`/`Pwrite` traits, ctx | none | explicit ctx | fast | byte-level, more manual than derive. |
| **zerocopy** | transmute (`FromBytes`/`IntoBytes`) | none (needs 2-stage validation) | none native (newtype ints) | fastest (no copy) | fixed `repr(C)` layouts only; no variable-length / bit packing. |
| **bincode/postcard** | own wire format | — | — | fast | **not** for arbitrary protocol layouts. |

Takeaway: **binrw and deku are the only two declarative bidirectional options.**
binrw is fast but bit-blind; deku is bit-aware but pays `bitvec`. You want
bit-aware **and** fast — which neither delivers, but a `bits` layer over binrw
does (native bit fields via integer shift/mask + binrw's fast byte stream).

### 3.2 Bitfield / int / enum helpers (the `bits` layer)

| Crate | Backing | Order | int types | binrw | Note |
|---|---|---|---|---|---|
| **bitbybit** | integer | bit index (range) | `arbitrary-int` | derive-able | clean, what `dns` uses |
| **bilge** | integer (splits big) | LSB-ish | `arbitrary-int` | manual | modern, ergonomic; arbitrary-int based |
| **modular-bitfield(-msb)** | byte array | LSB / **MSB** | own `B1..B64` | map glue | "as fast as handwritten" but needs glue |
| **bitfield-struct** | integer | LSB | primitive | derive-able | const-fn accessors |
| **arbitrary-int** | — | — | `u1..u127` | — | the sub-byte int substrate |
| **num_enum** | — | — | — | — | enum⇄int + `catch_all` |

All four bitfield crates do ~the same thing; the differences that matter to you
are **bit order** and **how they touch binrw**. None gives per-field byte order
*and* bit order *and* native binrw, which is the gap `bits` fills.

Sources: deku docs & binrw discussion #184/#222 (bit support), the deku/binrw
perf threads, and the bilge/modular-bitfield/zerocopy comparisons — see §8.

---

## 4. Build vs. buy: should we replace `binrw`?

**Not first, and probably not for a long time — but architect so we *can*.**

Arguments weighed:

- **Your pain is the bit layer and the glue, not the stream codec.** Every wart
  in §2 is about bit fields, ordering, and the binrw bridge. binrw's stream
  machinery is not the problem. Fixing the bit layer removes ~all of the pain
  for a fraction of the cost.
- **A codec is a large, bug-prone surface.** Read+Seek streaming, argument
  threading, magic, conditionals, restore-position, error spans, the `helpers`
  — binrw has years of hardening here. Reimplementing is months with a long
  tail, for little marginal benefit *today*.
- **The ecosystem proves the tradeoff is real, not solved.** deku chose
  bit-native + `bitvec` (slower); binrw chose byte-native + fast (bit-blind).
  Building `bits` over binrw gets you **both** sides: native bit fields without
  `bitvec`, on top of a fast stream.
- **But keep the door open.** Two limitations *could* eventually justify an
  in-house codec: (a) true sub-byte fields that straddle byte boundaries *in the
  stream* (today handled by collapsing to an integer first — fine for headers,
  awkward for exotic layouts); and (b) unifying the
  binrw+derive_builder+bitfield triad into a single `#[message]` derive. If
  those bite, a focused codec is justified — and cheap to adopt because of the
  seam below.

**Decision rule:** ship `bits` over binrw; define `bits::codec` traits as the
real interface; bridge to binrw. Re-evaluate an in-house codec only against a
concrete, recurring limitation — not speculatively.

---

## 5. Proposed `bits` design

### 5.1 Crate layout (fixes the current breakage)

```
bits/            # runtime lib (NOT proc-macro): types, traits, re-exports
bits-macros/     # proc-macro = true: #[bitfield], #[derive(BitEnum)], (later) #[message]
```
`bits` re-exports the macros so users write `use bits::*;`. Both join the
workspace. Macro code moves to `syn 2.0`. This alone resolves the
proc-macro/runtime contradiction and the syn-version mismatch.

### 5.2 `bits::int` — arbitrary-width unsigned ints (replaces `arbitrary-int`)

`u1, u2, … u127` as newtypes over the smallest fitting primitive, with checked
`new`/`try_new`, `From`/`TryFrom`, `MIN`/`MAX`, and the bit-width const. Used as
field types and as bitfield backings.

### 5.3 `#[bitfield]` — the core macro (replaces all four bitfield crates)

Integer-backed (backing chosen by total width: `u8…u128`), **shift/mask** codegen
(no `bitvec`), with explicit, orthogonal ordering:

```rust
use bits::{bitfield, u4, u5, u7};

// One declaration replaces the DNS/NBT/SMB variants above.
#[bitfield(u16, bits = msb, bytes = be)]   // MSB-first packing, big-endian on the wire
#[derive(Debug, PartialEq, Eq)]            // BinRead/BinWrite generated automatically (§5.5)
pub struct State {
    pub opcode: OpCode,   // nested 5-bit bitfield
    pub flags:  Flags,    // 7-bit
    pub rcode:  RCode,    // 4-bit BitEnum with catch-all
}
```

Field-level control where needed:
```rust
#[bitfield(u32, bits = msb, bytes = be)]
struct Example {
    #[bits(3)]            version: u3,            // width form
    #[bits(4..=7)]        kind:    Kind,          // range form (escape hatch)
    #[bits(1)]            df:      bool,
    #[bits(rest)]         payload_len: u24,       // "fill remaining"
    // per-field byte-order override is possible for embedded multi-byte sub-fields
}
```

Generated per type: getters, immutable `with_*` setters, `new()`/`Default`,
`raw()/from_raw()`, `TryFrom`/`Into` the backing int, `Debug`/`Display`. The
`with_*` API *is* a builder — it directly removes the
`State::new().with_x().with_y()` `calc` pain.

Key knobs:
- `bits = msb | lsb` — does field 0 occupy the **high** or **low** bits.
- `bytes = be | le` — endianness of the backing integer when (de)serialized.
- These are independent (you can have MSB-first bit packing in an LE integer).
- Nested bitfields and `BitEnum`s compose as field types and pack at their width.

### 5.4 `#[derive(BitEnum)]` — enum ⇄ int with catch-all (replaces `num_enum` + `bitenum`)

```rust
#[derive(BitEnum, Debug, PartialEq, Eq)]
#[bit_enum(u4)]                 // 4-bit on the wire
pub enum RCode {
    NoError = 0, FormErr = 1, ServFail = 2, /* … */
    #[catch_all] Custom(u4),    // dual-use: unknown values preserved, never reject
}
```
Implements the bitfield-specifier trait (so it nests), plus the codec traits at
its declared width. The `#[catch_all]` mirrors the workspace's dual-use
`Custom(..)` convention (and `num_enum`'s `catch_all`).

### 5.5 Native codec integration (the seam) — kills the `map` glue

Define the real interface in `bits`:
```rust
// bits::codec
pub trait Encode { fn encode<W: io::Write + io::Seek>(&self, w: &mut W, order: ByteOrder) -> Result<()>; }
pub trait Decode: Sized { fn decode<R: io::Read + io::Seek>(r: &mut R, order: ByteOrder) -> Result<Self>; }
```
The `#[bitfield]`/`BitEnum` macros implement `Encode`/`Decode`. A **binrw bridge**
(default-on `binrw` feature) provides `impl<T: bits::Encode> BinWrite for T` /
`impl<T: bits::Decode> BinRead for T` (or a generated `impl` per type to avoid
coherence issues). Result:

```rust
#[binrw]
#[brw(big)]
pub struct Header {
    pub transaction_id: u16,
    pub state: State,      // <-- no #[br(map)]/#[bw(map)]. It just works.
    pub qdcount: u16,
    // …
}
```

When/if an in-house codec lands, it consumes the same `bits::Encode`/`Decode`;
field types don't change. That is the entire point of the seam.

### 5.6 `derive_builder` interplay

- Short term: keep `derive_builder` for *outer* protocol structs; the bitfield
  `with_*` API removes the `calc` glue for the *inner* collapsed fields.
- Optional later: a `#[message]` derive in `bits-macros` that folds
  **binrw + builder + soundness-check** (the `check_soundness`/`#[brw(ignore)]`
  pattern) into one attribute — replacing the binrw+derive_builder+calc triad
  for headers. This is the natural place to eventually drop `derive_builder`.

### 5.7 Why this is fast (the stated constraint)

Integer shift/mask, fully monomorphized, no `bitvec`, no per-field heap, no
runtime field tables (unlike the *current* `bits/src/lib.rs` string-keyed
runtime `Bitfield`, which should be **discarded** in favor of codegen). Build
time may rise (more generated code) — explicitly acceptable per the goal.

---

## 6. Migration & dependency-drop plan

1. **Stand up `bits` + `bits-macros`** (fix layout, syn 2.0); port the int
   types, `#[bitfield]`, `BitEnum`, and the binrw bridge. Golden tests: round-trip
   every shape against hand-computed bytes, both bit orders, both byte orders.
2. **Prove it on `nbt`** (heaviest user: 3 bitfield crates + dead dep + version
   drift). Migrate `State`, `OpCode`, `Flags`, headers. Delete
   `modular-bitfield`, `modular-bitfield-msb`, `bitfield-struct`. Bump
   `binrw`/`derive_builder` to current.
3. **`dns`** → replace `bitbybit` + `arbitrary-int`. **`smb`** → replace
   `modular-bitfield`; keep `bytes` for now. **`arp`** → replace `num_enum`
   (catch-all) with `BitEnum`.
4. **Ripple out**; each crate's tests/protoref ledger guard the swap.
5. **Later/optional:** absorb byte-domain helpers — `bits::bytes` (buffers),
   `bits::checksum` (CRC32 + the Internet checksum already in `icmp`),
   `bits::mac` — to drop `bytes`/`crc32fast`/`macaddr`. Then evaluate the
   `#[message]` derive (drop `derive_builder`) and, last, an in-house codec
   (drop `binrw`).

**Dependency ledger**

| Drop now (Phase 1) | Keep (with exit path) | Orthogonal (absorb later) |
|---|---|---|
| arbitrary-int, num_enum, modular-bitfield, modular-bitfield-msb, bitfield-struct, bitbybit | binrw (codec seam), derive_builder (`#[message]`) | bytes, crc32fast, macaddr |

---

## 7. Open decisions for the maintainer

1. **Scope of Phase 1** — bitfields + ints + enums + binrw bridge (recommended),
   or also the `#[message]` derive in the first pass?
2. **`bits` vs `bits` + `bits-macros` split** — the split is required (proc-macro
   crates can't export runtime types); confirm the two-crate layout is fine.
3. **Bit-order default** — default `bits = msb` (network/RFC-diagram order, since
   most protocols here are MSB-first) with `lsb` opt-in? Or no default (always
   explicit)?
4. **binrw bridge as a feature** — keep `binrw` behind a default-on feature so
   `bits` is usable standalone and the eventual codec swap is a feature flip?
5. **Existing `bits/src/{lib,types}.rs`** — discard the runtime string-keyed
   `Bitfield` in favor of codegen (recommended), or preserve any of it?

---

## 8. Sources

- deku — docs.rs/deku; bit support & `bitvec` backing; perf threads
  (github.com/jam1garner/binrw/discussions/184, rust-lang/rust#118674).
- binrw — docs.rs/binrw; bit-level support discussion
  (github.com/jam1garner/binrw/discussions/222).
- bilge — github.com/hecatia-elegua/bilge; modular-bitfield — lib.rs/crates/modular-bitfield.
- zerocopy bitfield integration — github.com/google/zerocopy/issues/1497.
