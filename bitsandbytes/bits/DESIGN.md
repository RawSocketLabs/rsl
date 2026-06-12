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
  binrw+derive_builder+bitfield triad into a single `#[wire]` derive. If
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
bits-macros/     # proc-macro = true: #[bitfield], #[derive(BitEnum)], (later) #[wire]
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
- Optional later: a `#[wire]` derive in `bits-macros` that folds
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
   `#[wire]` derive (drop `derive_builder`) and, last, an in-house codec
   (drop `binrw`).

**Dependency ledger**

| Drop now (Phase 1) | Keep (with exit path) | Orthogonal (absorb later) |
|---|---|---|
| arbitrary-int, num_enum, modular-bitfield, modular-bitfield-msb, bitfield-struct, bitbybit | binrw (codec seam), derive_builder (`#[wire]`) | bytes, crc32fast, macaddr |

---

## 7. Open decisions for the maintainer

1. **Scope of Phase 1** — bitfields + ints + enums + binrw bridge (recommended),
   or also the `#[wire]` derive in the first pass?
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

## 8. `#[bitflags]` and a builder derive — **implemented** (2026-06-11)

Two additions on top of Phase 1, now shipped (`bits-macros/src/bitflags.rs`,
`builder.rs`; tests in `tests/{flags,builder}.rs`; example `tcp_segment`). Both
are additive, slot beside the existing macros, and **compose** with them (they
implement `Bits` + binrw, so a flags type is a valid `#[bitfield]` field and a
binrw field).

### 8.1 `#[bitflags]` — named single-bit flag sets

**Why.** The workspace has no `bitflags` crate; flag sets are modeled today as
*N separate `bool` fields* inside a `#[bitfield]` (NBT resource flags
`group`/`deregister`/`conflict`/`active`/`permanent`; DNS `recursion_desired`;
etc.). That gives per-flag get/set but no **set algebra** — you cannot write
`SYN | ACK`, test `flags.contains(RD | RA)`, or iterate the set bits. For
genuinely set-shaped fields (TCP flags, IPv4 DF/MF, capability bitmasks) the
bool-field approach is clumsy. This is a distinct primitive from `#[bitfield]`
(a *homogeneous set of named 1-bit flags with set algebra*, vs. a *struct of
heterogeneous typed fields*), and keeping it in `bits` avoids re-adding the
external `bitflags` dependency — while adding what `bitflags` lacks: `Bits` +
binrw integration so flags nest and serialize.

**Proposed shape** (attribute style, consistent with `#[bitfield]`). An attribute
macro decorates a *real* struct, and struct fields require types, so each flag is
a `bool` field (which also reads honestly — a `bool` is a 1-bit flag); bit
positions auto-assign in declaration order, with `#[flag(N)]` to fix one
explicitly. Combinations are plain consts built with the generated const helpers.

```rust
#[bitflags(u8)]                    // backing primitive; `bytes = be|le` optional
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TcpFlags {
    fin: bool,                     // value 1 << 0 (auto, LSB-indexed)
    syn: bool,                     // 1 << 1
    rst: bool, psh: bool, ack: bool, urg: bool, ece: bool, cwr: bool,
}
impl TcpFlags {
    pub const HANDSHAKE: Self = Self::SYN.union(Self::ACK); // a combination const
}

let f = TcpFlags::SYN | TcpFlags::ACK;
assert!(f.contains(TcpFlags::SYN));
assert!(f.syn());                  // per-flag bool accessor
for one in f.iter() { /* each set single-bit flag */ }
```

**Generated:** per-flag `const FIN: Self` (etc.) and any combination consts;
`empty()`, `all()`, `bits()`, `from_bits` (dual-use: **retains** unknown bits),
`from_bits_truncate`, `contains`, `intersects`, `insert`/`remove`/`toggle`/`set`,
`is_empty`, `iter()`; the `BitOr`/`BitAnd`/`BitXor`/`Not` (+ assign) operators;
and `Bits` + binrw impls.

**Defaults I'd pick (override in §8.3):** flags are **value-based / LSB-indexed**
(flag *n* = `1 << n`; the universal convention — `bits = msb` does not apply to a
flag set), `from_bits` **retains** unknown bits (dual-use), `iter()` yields only
the **single-bit** named flags that are set (not combination consts), and field
names are upper-cased into consts (`fin` → `TcpFlags::FIN`, the `bitflags`
idiom).

### 8.2 A builder derive (`#[derive(BitsBuilder)]`)

**Why.** Today protocol structs pair `#[derive(Builder)]` (derive_builder) with
binrw, and to assemble a collapsed bitfield from individual builder fields they
resort to `#[bw(calc = State::builder().with_opcode(self.opcode)…build())]` plus
`#[builder(default, setter(into))]` and `#[brw(ignore)]` builder-only fields
(DNS `Header`, NBT `Header`). It works but is clumsy, and `#[bitfield]`'s infix
`new().with_*()` is *infallible* — it silently defaults an unset field to 0, so
"you forgot to set the opcode" is not caught. A `bits`-native builder adds the
derive_builder value — **required-field tracking** — to bit/byte structs, which
is the "call out setting or not setting individual bits/bytes" the maintainer
wants.

**Proposed shape** — a separate `FooBuilder` (Option-backed per field, the
derive_builder analog) generated alongside the infix `with_*`:

```rust
#[bitfield(u16, bits = msb)]
#[derive(BitsBuilder, Clone, Copy)]
struct State {
    opcode: u4,                    // required by default
    #[builder(default)]            // optional; 0 if unset
    flags: u8,
    rcode: RCode,                  // required
}

let s = State::builder()
    .opcode(u4::new(2))
    .rcode(RCode::ServFail)
    // flags omitted -> default
    .build()?;                     // Err(MissingField) if opcode/rcode unset
```

**Generated:** `State::builder() -> StateBuilder`; fluent `.field(value)` setters
(Option-tracked); `.build() -> Result<State, BuilderError>` that errors listing
any unset **required** field; per-field `#[builder(default)]` / `#[builder(default = expr)]`
to make a field optional. The builder distinguishes *set* from *unset* — that
tracking is the feature. It coexists with the infix `with_*` (quick path) and is
intended to **replace derive_builder** on bit/byte structs, feeding the
eventually-folded `#[wire]` derive (binrw + builder + soundness, §5.6).

**Defaults I'd pick (override in §8.3):** **required-by-default** (every field
must be set unless `#[builder(default)]`, like derive_builder) so `build()`
enforces completeness; a **separate** `FooBuilder` type with `build() -> Result`
(not an enhancement of the infallible `with_*`); scope = **`#[bitfield]` types
first**, extended to plain "message" structs (bitfields + plain byte fields) in a
later pass.

### 8.3 Decisions (confirmed 2026-06-11)

1. **bitflags accessors** — set algebra **and** per-flag bool accessors. In
   addition to the consts/operators/`contains`/`iter`, generate `fin(&self) -> bool`
   getters, `with_fin(bool) -> Self`, and `set_fin(bool)` (field-named, matching
   `#[bitfield]` getters), so the current bool-field clusters migrate directly.
2. **bitflags indexing** — value-based / LSB (`flag n = 1 << n`), with explicit
   `= 1 << k` and combination (`= SYN | ACK`) overrides. No `msb` flag mode.
3. **builder scope** — `#[bitfield]` types first this round; plain "message"
   structs (bitfields + byte fields) come with the later `#[wire]` derive.
4. **builder semantics** — required-by-default; a field must be set or
   `build()` returns `Err` naming the missing field(s). `#[builder(default)]` /
   `#[builder(default = expr)]` makes a field optional. A **separate**
   `FooBuilder` (Option-tracked) with `build() -> Result`, coexisting with the
   infallible infix `with_*`.
5. **builder placement** — a standalone `#[derive(BitsBuilder)]` in the derive
   list. **Mechanism note:** because `#[bitfield]` rewrites the struct to a single
   backing integer *before* derives run, a real derive can no longer see the
   logical fields. So `#[bitfield]` **intercepts** `BitsBuilder` from its own
   derive list (where the fields are still visible), generates the builder, and
   strips the marker so nothing runs on the collapsed struct. It reads as a normal
   `#[derive(BitsBuilder)]` to the user. (A real `BitsBuilder` derive also exists
   for plain, non-`#[bitfield]` structs — the seed of the §8.2 message-struct
   extension.)

---

## 9. The `#[wire]` macro (implemented 2026-06-11)

A single attribute that folds the protocol-header triad — **binrw codec +
builder + collapsed bit-groups + derived fields + soundness** — that DNS/NBT
headers stack by hand. It is *sugar that expands to the existing primitives*
(`#[binrw]` + `#[derive(BitsBuilder)]` + `#[bitfield]`), not a new codec.

### 9.1 Research: the feature surface and the sharp edges

**Workspace binrw usage** (attribute frequency across all crates): `magic` ×214,
`pre_assert` ×84, `big/little` ×84, `map` ×46, `count` ×32, `args` ×25,
`import` ×19, `ignore` ×12, `calc` ×9, `parse_with` ×7, `if` ×3,
`restore_position` ×2, `temp`/`try_calc` ×1. `derive_builder`: `default` ×50,
`setter` ×17, `validate` ×15.

**Conclusion → wrap binrw, don't replace it.** Reimplementing this surface
(magic/pre_assert/count/args/parse_with/…) would be enormous *and* would destroy
the escape hatch. So `#[wire]` emits a `#[binrw]` struct and **passes every
`#[br]/#[bw]/#[brw]` attribute through untouched**, adding native sugar only for
the patterns binrw expresses awkwardly. (An in-house codec — DESIGN §4's 2b —
stays deferred; `#[wire]` does not force it.)

**The real shapes it must fold** (DNS/NBT `Header`):
1. *Collapsed bit-group*: one wire integer, N named fields — today a private
   `state` field with `#[bw(calc = State::builder()…)]` to reassemble on write,
   plus N fields each `#[bw(ignore)] #[br(calc = state.x())]` to disassemble on
   read. Six+ attributes and a private field, kept in sync by hand.
2. *Derived fields*: `qdcount` etc. `#[bw(map = |x| x.unwrap_or(queries.len()))]`
   + a struct-level `#[bw(import(queries, …))]`.
3. *Count-driven `Vec`s*: `#[br(count = questions)] Vec<Question>` (pure binrw).
4. *Builder-only fields*: NBT `#[brw(ignore)] #[builder(default = true)] check_soundness`.
5. *Soundness*: `derive_builder`'s `build_fn(validate = "Self::validate")`.

**Ecosystem sharp edges to design around:**
- **binrw `temp` does not cross the read/write boundary** (binrw #47): a field
  read-only on `br` is not automatically calc-only on `bw`. This is *exactly*
  why the hand-written headers carry matched-but-separate `#[br(calc)]` /
  `#[bw(calc)]` that can drift. `#[wire]` **generates the matched pair**, so
  the two directions can't diverge — a concrete correctness win, not just terser.
- **Proc-macro span loss**: re-emitted tokens lose spans, so errors point at the
  attribute, not the field. Mitigation: emit field tokens with their **original
  spans**, and report misuse via well-spanned `compile_error!`
  (`syn::Error::new_spanned`), never a macro panic. Test illegal programs (a
  `trybuild` compile-fail suite) so error quality is regression-guarded.
- **Nested attribute macros** (`#[wire]` emitting `#[binrw]`): valid and
  common, but the emitted `#[binrw]` must be well-formed; keep generated glue
  minimal and span-correct.
- **deku** (the most complete reference) names the same surface: `update`
  (compute-on-write), `temp`, `assert`/`assert_eq`, `cond`, `ctx`, `pad_*`,
  `id`. We mirror the useful ones (`update`, validation) and lean on binrw
  passthrough for the rest.

### 9.2 Native features (the value over raw binrw + builder)

```rust
#[wire(big)]                       // endianness; wraps #[binrw] #[brw(big)]
struct Header {
    id: u16,                          // plain field: builder + binrw field

    #[group(u16)]                     // (1) inline bit-group -> one u16 on the wire,
    opcode: OpCode,                   //     opcode/flags/rcode first-class in the
    flags:  Flags,                    //     builder & as accessors; matched br/bw
    rcode:  RCode,                    //     glue generated (no drift)

    #[update(queries.len() as u16)]   // (2) derived on write; overridable
    qdcount: u16,

    #[builder(default)]               // ESCAPE HATCH: builder attr passes through
    #[br(count = qdcount)]            // ESCAPE HATCH: binrw attr passes through
    queries: Vec<Question>,

    #[builder_only(default = true)]   // (4) not on the wire; gates soundness
    check_soundness: bool,
}

#[wire(big, validate = Header::soundness)]   // (5) run in build() + post-read,
                                                //     gated by check_soundness
```

- **`#[group(uN)]`** — the killer feature: a run of fields packed into a `uN` on
  the wire, exposed individually. Generated as `#[br(temp)]` packed read +
  `#[bw(calc = pack)]` + sub-field `#[br(calc = unpack)]` + `#[bw(ignore)]`.
  Smooths the temp-cross-boundary edge.
- **`#[update(expr)]`** — compute-on-write (counts/lengths/checksums); the field
  is written from `expr`, so you never hand-thread `import`.
- **`#[builder_only]`** — builder field, not on the wire.
- **soundness `validate`** — runs in `build()` and after `BinRead`, gated by a
  `#[builder_only]` bool, so disabling it is the documented dual-use escape hatch
  for emitting malformed traffic.
- **builder** — `BitsBuilder`-style, folded in.

### 9.3 Escape hatches & integration (a hard requirement)

- **Every `#[br]/#[bw]/#[brw]` attribute passes through verbatim** — the full
  binrw surface (magic, count, args, import, map, parse_with, if, pre_assert,
  restore_position, pad, …) remains available on any field.
- **Every `#[builder(...)]` attribute passes through** to the generated builder.
- A `#[wire]` struct **is** a `#[binrw]` struct — it produces standard
  `BinRead`/`BinWrite`, so every binrw consumer (and `bits` bitfields/enums/flags)
  composes unchanged. You can always drop to raw `#[binrw]` + `#[derive(BitsBuilder)]`.
- `#[wire]` requires the `binrw` feature (it wraps binrw); document that.

### 9.4 Decisions (confirmed 2026-06-11)

1. **Group syntax — struct-level, named, order-sensitive.** Groups are declared
   in the attribute by **field name**: `#[wire(big, group(opcode, flags, rcode => u16))]`
   (multiple `group(...)` clauses allowed; the backing `uN` is the wire size).
   Naming the fields means a moved/renamed field is a **compile error**, not a
   silent mislayout. The macro **enforces** that the named fields appear
   **consecutively and in the same order** in the struct body, erroring (well
   spanned) otherwise. (No per-field tags — the user found those unclear about
   *which* group, and the struct-level form gives the size one place.) Mechanism:
   the macro generates a private `#[bitfield(uN, bits = msb, bytes = <endian>)]`
   for the group and wires it with `#[br(temp)]` (read the packed word into a
   temp) + `#[bw(calc = Group::new().with_…())]`, and turns each member into a
   stored `#[br(calc = temp.field())] #[bw(ignore)]` field — so the matched
   read/write pair is generated together and cannot drift (the binrw #47 fix).
2. **`#[update(expr)]` — always recompute on write.** The field becomes
   `#[br(temp)] #[bw(calc = expr)]`: not stored, not in the builder; on read it is
   a temp (usable by a later `#[br(count = field)]`), on write it is always `expr`.
   Derived values can never disagree with the payload.
3. **Builder — always, opt out** with `#[wire(no_builder)]`.
4. **Soundness — `#[wire(validate = path)]`.** Auto-creates a `check_soundness`
   builder-only flag (default `true`); `build()` runs the validator (`path:
   fn(&Self) -> Result<(), impl Display>`, surfaced as `BuilderError::Invalid`)
   when the flag is set; setting it `false` is the dual-use escape hatch. A
   `validate(&self)` method is generated for **opt-in** post-parse checking.
   **The parser (`BinRead`) is deliberately left permissive** — it never rejects
   representable input. This is the workspace's dual-use rule (CLAUDE.md: "never
   enforce a policy requirement inside a parser"): validation is a
   construction-time / opt-in concern, not a parser concern. Auto-validating on
   read would both violate that rule and (via a reconstruct-and-clone hack) tax
   the zero-cost `BinRead` path — so it is intentionally not done.

### 9.5 Outcome

Implemented in `bits-macros/src/wire.rs` (gated on the `binrw` feature; the
dependent crate needs `binrw` as a direct dep). Lowers to a `#[binrw]` struct +
a private `#[bitfield]` per group + a `BitsBuilder`-style builder, reusing the
existing generators. `BuilderError` grew an `Invalid(String)` case so a
validator's error of any `Display` type flows through `build()` without coupling.
Tested in `bits/tests/wire.rs` (groups, derived `#[update]` counts,
count-driven `Vec`s, `#[builder_only]`, multi-group, little-endian, `no_builder`,
soundness dual-use, and binrw `map`/`magic` passthrough — plus a capstone using
every feature) and `bits/tests/ui/*` (10 compile-fail cases proving group
misuse — non-adjacent, out-of-order, unknown, duplicate, marker conflicts,
under-filled group, generic struct — is caught with well-spanned errors).
Example: `bits/examples/wire_header.rs`.

### 9.6 Production hardening (stress testing)

Surfaced and fixed while stress-testing:

- **A group must fill its backing exactly.** If the members' widths sum to fewer
  bits than the backing, the underlying `#[bitfield]` silently right-aligns them
  (padding the *high* bits) — a latent wire bug. The macro now emits a const-eval
  assertion (`Σ member BITS == backing BITS`) so an under-/over-filled group is a
  compile error pointing at the group; model intentional padding as an explicit
  `reserved` member. (`#[allow(clippy::identity_op)]` keeps the generated sum from
  warning in the user's crate.)
- **Generics/lifetimes are rejected** with a clear message (they aren't threaded
  into the generated group bitfields/builder), rather than emitting a struct that
  silently drops them.

Stress coverage added:

- `tests/wire_proptest.rs` — property-based round-trips: `encode∘decode = id` over
  random field values (incl. catch-all enums and variable sections), and
  `decode∘encode = id` over random bytes (the parser is total and the group word
  is a bijection).
- `tests/wire_golden.rs` — real DNS header byte-vectors (RFC 1035 §4.1.1, flags
  word as an 8-member group): standard query, authoritative NXDOMAIN response, and
  opcode-in-high-bits — byte-for-byte.
- `tests/wire_stress.rs` — edge matrix: little-endian multi-byte group, back-to-back
  groups, a nested `#[bitfield]` as a group member, `builder_only` without a
  default, a user-declared `check_soundness` (no double-insert), `validate` +
  `no_builder`, a custom `Display` validator error, `#[wire]`-in-`#[wire]`, and
  group-type-name disambiguation across two structs.

**Not yet done (the decisive test):** wiring `#[wire]` into one real header (e.g.
`nbt`) and asserting byte-identical output + the crate's existing tests pass —
gated by "don't migrate yet."

---

## 10. Sources

- deku — docs.rs/deku; bit support & `bitvec` backing; perf threads
  (github.com/jam1garner/binrw/discussions/184, rust-lang/rust#118674).
- binrw — docs.rs/binrw; bit-level support discussion
  (github.com/jam1garner/binrw/discussions/222).
- bilge — github.com/hecatia-elegua/bilge; modular-bitfield — lib.rs/crates/modular-bitfield.
- zerocopy bitfield integration — github.com/google/zerocopy/issues/1497.
- deku attribute reference (update/temp/assert/cond/ctx/pad/id) — docs.rs/deku/latest/deku/attributes.
- binrw temp does not cross read/write — github.com/jam1garner/binrw/issues/47.
- proc-macro error reporting / span loss — blog.turbo.fish/proc-macro-error-handling,
  rust-lang/rust#76360, dtolnay/proc-macro2#104.
