# `bnb` — design rationale

`bnb` is an owned, bit-aware binary codec: arbitrary-width integers, bitfields,
enum⇄integer mappings, flag sets, a required-by-default builder, and a unified
`#[bin]` whole-message codec — one crate, integer-backed, fast. This document is the
design rationale: the problem it solves, the shape of the solution, and the decisions
behind it. For runnable walkthroughs see the [`bnb::guide`] module; for credit see
[`ACKNOWLEDGMENTS.md`](ACKNOWLEDGMENTS.md).

[`bnb::guide`]: https://docs.rs/bnb/latest/bnb/guide/

## 1. The problem

From-scratch protocol implementations need to pack and parse binary layouts that are,
all at once:

- **bit-aware** — fields narrower than a byte (a 4-bit opcode), odd widths (a 12-bit
  length), and fields that straddle byte boundaries (a 108-bit payload at bit 8);
- **order-explicit** — independent control of *bit order* (does the first field land
  in the high or low bits — MSB-first matches RFC diagrams) and *byte order*
  (big/little), because protocols mix them;
- **fast** — shift/mask on machine integers, no `bitvec`-style bit-vector backing;
- **dual-use** — RFC-correct by default, but deliberately able to emit and accept
  non-conformant data for fuzzing, red-teaming, and interop testing.

No single existing crate delivers all four. The capabilities were previously spread
across a stack of overlapping helpers — `arbitrary-int` (sub-byte ints),
`modular-bitfield`/`bitfield-struct`/`bitbybit` (bitfield packing, each with a
different bit-order convention), `num_enum` (enum⇄int) — glued to a byte-oriented
codec by hand. The glue was the pain: a byte-oriented `Read + Seek` codec has no idea
bit fields exist, so every bitfield needed `map`-style conversion glue, and the same
16-bit `opcode/flags/rcode` header would be implemented three different ways in three
protocols. `bnb` collapses that stack into one coherent, bit-native crate.

## 2. Inspiration, and why an owned codec

`bnb` is modeled on [`binrw`](https://github.com/jam1garner/binrw): its declarative,
bidirectional read/write derive and its `#[br]`/`#[bw]`/`#[brw]` attribute vocabulary
are the design `#[bin]` echoes, so a `binrw` user is immediately at home. The
arbitrary-width integers, bitfield packing, and enum mapping draw on `arbitrary-int`,
`modular-bitfield`/`bitfield-struct`/`bitbybit`, and `num_enum` respectively. `bnb`
shares no code with any of them — it is a from-scratch implementation.

The landscape that motivated building rather than composing:

| Approach | Bit fields | Speed | Note |
|---|---|---|---|
| byte-oriented derive codec (`binrw`-style) | none (needs glue) | fast, byte-aligned | mature stream machinery, but bit-blind |
| bit-aware derive codec (`deku`-style) | native | slower (`bitvec` backing taxes even byte paths) | one crate, but pays the bit-vector cost |
| parser combinators (`nom`) | via `nom::bits` | fast | parse-only, not declarative bidirectional |
| transmute (`zerocopy`) | none | fastest | fixed `repr(C)` only; no variable-length/bit packing |

The two declarative bidirectional options sit on opposite horns: byte-native is fast
but bit-blind; bit-native is bit-aware but pays a bit-vector tax. The decisive case is
a layout like a **DMR burst** — 264 bits = `108 | 48 | 108`, none byte-aligned. A byte
cursor can only address byte boundaries, so such a field forces hand-rolled backward
seeks and nibble shifts; a bit-vector codec handles it but is slow everywhere. `bnb`
takes a third path: a **bit cursor over machine integers** — bit-aware *and* fast
(shift/mask, no `bitvec`). That capability is the reason the codec is owned rather than
layered on a byte-oriented one.

## 3. Architecture

### 3.1 One keystone: the `Bits` trait

Everything composes through one trait — a value that occupies a fixed number of bits:

```rust
pub trait Bits: Copy {
    const BITS: u32;
    fn into_bits(self) -> u128;       // the value in the low BITS bits of a u128
    fn from_bits(raw: u128) -> Self;  // reconstruct from the low BITS bits
}
```

`bool`, the primitive unsigned integers, the `u1`..`u127` arbitrary-width integers, and
every type the macros generate (`#[bitfield]`, `#[derive(BitEnum)]`, `#[bitflags]`) all
implement `Bits`. Because the unit of composition is "a value of N bits," a 5-bit enum
nests in a 16-bit bitfield which nests in a byte-aligned `#[bin]` message — without any
glue, and with widths checked by the compiler.

A `Bits` value is the unit of bit-*packing*, but it is also a unit of *stream* coding:
every `Bits` type additionally implements the message codec traits
(`BitDecode`/`BitEncode`/`FixedBitLen`) as thin delegations to reading/writing its bits.
So `#[bin]` decodes, encodes, and sizes every field — a `Bits` leaf *or* a nested `#[bin]`
message — through one uniform interface, with no marker to disambiguate the two (see §8).

### 3.2 Two crates

A proc-macro crate cannot also export runtime items, so:

- **`bnb`** — the runtime: `int` (`UInt<T, N>` + `u1..u127`), `field` (`Bits`,
  `Bitfield`, `BitOrder`, `ByteOrder`), `error`, `builder`, and `bitstream` (the codec
  runtime — cursors, traits, the I/O ladder). Re-exports the macros, so users depend
  only on `bnb`.
- **`bnb-macros`** — `#[bitfield]`, `#[derive(BitEnum)]`, `#[bitflags]`,
  `#[derive(BitsBuilder)]`, the low-level `#[derive(BitDecode/BitEncode)]`, and `#[bin]`.

### 3.3 The load-bearing macro idea: const-eval widths

A `#[bitfield]` cannot know the numeric width of a field whose type is another
bitfield or enum — that width lives in `<T as Bits>::BITS`, resolved by the compiler.
So instead of computing offsets itself, the macro emits **const expressions**
(`<T as Bits>::BITS`, cumulative sums, offset/mask arithmetic) that the compiler
evaluates during const-eval; the generated accessors then shift/mask the single
backing integer. The same principle sizes a fixed message's `FixedBitLen::BIT_LEN`.
The proc-macro never guesses a width — the compiler does the arithmetic, and an
impossible layout is a compile error rather than a silent miscompile.

## 4. Field types and macros

- **`u1`..`u127`** (`UInt<T, N>`) — range-checked sub-byte integers backed by the
  smallest sufficient primitive. Checked (`try_new`), panicking (`new`), and masking
  (`from_raw`) constructors.
- **`#[bitfield]`** — packs typed `Bits` fields into one backing integer, with
  `bits = msb|lsb` and `bytes = be|le` as independent knobs, and inferred /
  `#[bits(N)]` / `#[bits(A..=B)]` (manual range) width forms. Generates getters,
  immutable `with_*` setters, in-place `set_*`, and allocation-free `*_bytes`
  conversions.
- **`#[derive(BitEnum)]`** — enum ⇄ integer at a chosen width. A `#[catch_all]`
  variant preserves unknown values (the dual-use convention); without one, the enum
  must cover its whole width or be marked `#[bit_enum(uN, closed)]` (otherwise it is a
  compile error, since the infallible decode path would have nowhere to put an unknown
  value). A byte-aligned width also gets `num_enum`-parity `From`/`TryFrom`.
- **`#[bitflags]`** — a named set of single-bit flags with full set algebra
  (operators, `contains`/`iter`, per-flag accessors), dual-use retain-vs-truncate.
- **`#[derive(BitsBuilder)]`** — a required-by-default builder whose `build()` names
  the first unset field, closing the gap the infallible `with_*` setters leave (a
  field you forget is silently zero). `#[builder(default)]` / `#[builder(default = e)]`
  opt a field out.

## 5. The `#[bin]` codec

`#[bin]` folds the read codec, the write codec, and a required-by-default builder over
one struct, generating the decode entry points (`decode`/`peek`/`decode_exact`/
`decode_from`), the encode entry points (`to_bytes`, plus `to_canonical_bytes` for a
message that has a `reserved`/`calc` field — see §5.2), the `encode(writer)` convenience
(and `BitEncode::bit_encode` for writing into a `Sink`), and construction
(`Type::new(fields…)`, `Type::builder()`). Fields are read and written at arbitrary bit
offsets, so the same attribute handles byte-aligned headers and sub-byte frames, and any
`Bits` type *or* nested `#[bin]` message drops in as a field — both decoded, encoded, and
sized through one uniform codec path, with no marker (§8).

**Struct-level options:** `big`/`little`, `bit_order = msb|lsb`, `magic = <expr>`
(a leading constant verified on read, emitted on write — any `Bits` value, so it can be
sub-byte), `read_only`/`write_only`, `no_builder`, `forward_only`, `ctx(name: Ty, …)`,
and `validate = <path>`.

**Field directives** (`#[br]`/`#[bw]`, the inherited vocabulary): `count`, `ctx { … }`,
`temp` + `calc`, `if(…)`, `map`/`try_map` (+ inverse `bw(map)`),
`parse_with`/`write_with`, `ignore`, `pad_*`/`align_*`, `restore_position`, and
`#[reserved]`/`#[reserved_with(…)]`.

`#[bin]` lowers to `#[derive(BitDecode, BitEncode, BitsBuilder)]`; those bare derives
are the codec without the builder/`#[bin]` sugar, and they carry a **right-tool guard**
— a const-eval assert that rejects an all-byte-aligned struct (the cursor never leaves
byte boundaries, so `#[bin]` is the better tool, and a sub-byte run that fills one
integer wants `#[bitfield]`). The guard is advisory steering, with
`#[bit_stream(allow_byte_aligned)]` as the escape hatch; `#[bin]` always suppresses it.

### 5.1 Tagged-union enums

`#[bin]` also applies to an *enum* — a protocol union that selects one of several
payloads. The design keeps two concerns deliberately **orthogonal**, because protocols
mix them:

- **`magic`** — a wire constant that is *read and written* (a byte string like
  `b"IHDR"`, or a width-suffixed integer like `0x01u16`). Under magic dispatch it *is*
  the discriminant; combined with a tag it is a post-selection signature.
- **`tag`** — a read-only **selector** drawn from `ctx`, never on the wire. The parent
  passes it down (`#[br(ctx { … })]`); `tag()` recovers it to drive a no-drift `calc`.

The two compose, and may be mixed in one enum (tag priority, then magic) — the same
wire-constant-vs-selector split `#[bin]` draws on the struct side. The dual-use rule
carries over verbatim: a `#[catch_all]` variant preserves an unknown discriminant
rather than rejecting it; only an explicitly *closed* magic set errors. Variable-width
byte-string magics reuse the same [`SeekSource`](#6-the-io-ladder) capability the
positioning directives need — the bit cursor does the peeking, not a parallel mechanism.
The worked encodings live in the `bnb::guide::dispatch` page.

The encode model and construction surface below (§5.2) are **struct-only** — a
tagged-union enum encodes verbatim (no `to_canonical_bytes`/`encode_mode`/`validate`/`new`).
Those are properties of a concrete record; an enum's per-variant payloads define them, not
the union.

### 5.2 Encode model, construction, and validity

A message has two **encode forms**. `to_bytes` is **verbatim** — exactly what's stored, so
`decode → to_bytes` round-trips byte-for-byte and a deliberately-wrong field goes on the
wire as-is (dual-use). `to_canonical_bytes` is **canonical** — `reserved` fields written as
their spec value and `calc` fields recomputed, always spec-compliant. The two differ only
when a message has a `reserved` or non-`temp` `calc` field (a `temp`+`calc` field is never
stored, so it always recomputes and creates no gap), so `to_canonical_bytes` and the
in-memory helpers `to_canonical`/`canonical_diff`/`is_canonical` are generated only then.

The verbatim/canonical choice can also be **carried on the value**: such a message gains a
wire-ignored `encode_mode` field (default `Verbatim`; set via the builder, `with_encode_mode`,
`set_encode_mode`). It is consulted by exactly one entry point — the `std`-writer
`encode(w)` — so you can set the policy once and stream the value; the explicit `to_bytes`/
`to_canonical_bytes` (and the `BitEncode` sink methods `bit_encode`/`canonical_bit_encode`)
ignore it. The field is **excluded from `PartialEq`/`Eq`/`Hash`/`Debug`** (a render preference, not
data — `#[bin]` intercepts those derives), which means such a type is constructed via the
builder, `new(fields…)`, or `decode` rather than a struct literal.

`validate = path` (the construction-soundness check `build()` runs) is also exposed as
re-runnable `validate()` / `is_valid()` methods: `build()` checks once, but a value can be
mutated before sending, so these re-check the *current* value (computed, never a stored
flag). By convention `validate` expresses **semantic** soundness — not the representational
`calc`/`reserved` fields — so validity holds for the canonical form too; `to_canonical_bytes`
stays a pure normalization (compose `validate()` before sending if you want the check).

## 6. The I/O ladder

The everyday entry points work on byte slices and `Vec`s. For other inputs,
`decode_from` takes any `Source`, and `BitEncode::bit_encode` writes into any `Sink`:

| Source | Backing | Seek | Use |
|---|---|---|---|
| `BitReader` | `&[u8]` | free (cursor math) | in-memory bytes |
| `StreamBitReader` | any `Read` | no (forward only) | a stream read once |
| `BufSource` | any `Read` | yes (bounded retain-and-seek) | a socket that also seeks |
| `SeekReader` | `Read + Seek` | yes (via `io::Seek`) | a large file/container |
| `BytesReader`/`Writer` (`bytes` feature) | owned `Bytes` | yes | zero-copy async framing |

Seeking is only needed by a message that uses `restore_position`; everything else runs
over the forward-only `StreamBitReader` too. **Seeking is a source capability, enforced
in the type system:** when a message uses `restore_position`, the generated
`decode_from` is bound on `SeekSource`, so decoding it through a forward-only stream is
a compile error rather than a runtime surprise; `forward_only` is the opt-in that
forbids seek directives outright. The in-memory cursor needs no `Seek` trait at all —
the whole buffer is in hand, so a seek is just cursor arithmetic (which also enables
e.g. DNS name-compression pointer following).

### 6.1 `no_std` and the `std` feature (Option A)

`bnb` is `no_std` + `alloc`. `alloc` is unconditional — the codec's output model *is*
`Vec<u8>` (and `count` payloads / error strings own heap), so a heapless variant would
be a different crate, not a feature. The default-on **`std`** feature adds only the
rows of the table above that are backed by `std::io` (`StreamBitReader`/`BufSource`/
`SeekReader`, the `as_read`/`as_write` views), the `From<std::io::Error>` bridge +
`ErrorKind::Io`, and the `encode(writer)` convenience. The
forward-only/seekable distinction is unchanged; `no_std` simply has fewer `Source`
implementations to feed `decode_from` (the in-memory `BitReader`, and `BytesReader`
under `bytes`).

The chosen boundary is **buffer-at-a-time, not streaming** ("Option A"): `no_std`
decodes from a `&[u8]` and encodes to a `Vec<u8>`, then the caller writes those bytes to
its transport. This fits the workspace's datagram-oriented protocols (a UDP/ICMP/DNS
packet arrives whole) and keeps the change small and dependency-light. Two consequences
fall out of *a proc-macro cannot see the consumer crate's feature flags*:

- **`encode(writer)` is a blanket extension trait, not a generated inherent method.**
  The single `EncodeExt` is `std`-gated and blanket-implemented over `BitEncode`, so it
  appears exactly when `bnb/std` is on — whereas a `#[cfg(feature = "std")]` emitted into a
  generated method would key off the *user crate's* feature name and silently vanish for a
  default `cargo add bnb`. It dispatches to `bit_encode` vs `canonical_bit_encode` by the
  value's [`encode_mode`](EncodeMode) — a settable, wire-ignored field that a `reserved`/`calc`
  message carries (default `Verbatim`), rather than a call-time argument: set the policy once
  (builder/`with_encode_mode`) and stream the value. Both the canonical path and `encode_mode`
  are **defaulted methods on `BitEncode`** (no separate `CanonicalEncode` trait), so `encode`
  works for every message — one without `reserved`/`calc` has no field and stays verbatim.
  `#[bin]` intercepts `Debug`/`PartialEq`/`Hash` so the mode never affects equality (a render
  preference, not data), which makes these types builder/`decode`-constructed. Cost: callers
  bring the trait into scope (`use bnb::prelude::*`); the `to_bytes`/`to_canonical_bytes` `Vec`
  encoders stay inherent and unconditional (sink-writing uses the `BitEncode` trait methods).
- **`BitEncode` carries `const LAYOUT`** so the blanket `encode` can build a correctly
  ordered `BitWriter` without the per-type layout literal the old inherent method had.
- **`#[br(dbg)]` is `std`-only.** It emits a `tracing` event, and `tracing`'s default
  features link `std`; the workspace dep can't be overridden per-member, so `tracing` is
  an optional dep pulled in by `bnb`'s `std` feature. An embedded build uses its own
  logger. **A future "Option B"** (an in-house `bnb::io` `Read`/`Write`/`Seek`
  abstraction, à la `embedded-io`) would unify the code path and bring streaming to
  `no_std`; it is deferred until an embedded byte-stream transport (TCP/serial) needs it.

## 7. Dual-use by default

The crates are **compliant by default, deliberately violatable**:

- **Builder defaults are compliant**, but the fields stay settable.
- **Parsers accept representable-but-non-compliant values** — unknowns are modeled as
  data (`#[catch_all]`, retained flag bits), never hard errors.
- **Policy lives on the construction path, never in a parser.** `validate` gates
  `build()`; decoding stays permissive, so hostile input can be parsed for analysis
  but a malformed message can't be accidentally *built*.
- **Raw constructors never validate** (`from_raw`/`from_bits`, the `pub`-field struct
  literal) — the open escape hatch.

Only the *physically unencodable* is refused (a value that doesn't fit its field's
bits), never the merely non-conformant. The one place a decode can panic is a `closed`
enum fed an out-of-set discriminant — which is exactly why `closed` is an explicit
opt-in and the default for untrusted input is `#[catch_all]`.

## 8. Key implementation decisions

- **The `temp` + `calc` anti-drift pattern.** A length/count you don't want to store is
  read into a `#[br(temp)]` local and recomputed on write via `#[bw(calc = …)]`, so the
  two directions are generated together and can never disagree with the data they
  describe.
- **`validate` is construction-side only.** Auto-validating on decode would violate the
  dual-use rule (never reject representable input), so a soundness check runs in
  `build()` and surfaces as `BuilderError::Invalid`; the parser stays permissive.
- **No untrusted pre-allocation.** A `count`-driven `Vec` grows by pushing (each
  element consumes ≥1 bit), so an attacker-controlled count can't trigger a giant
  up-front allocation — it simply runs out of input and returns an error.
- **Context in two layers.** `ctx(...)` lowers to inherent `decode_with`/`encode_with`
  (Layer 1 — covers nesting, counts, borrowed context with no `Args` type on the core
  trait); a `DecodeWith<A>`/`EncodeWith<A>` companion (Layer 2) carries the same to
  hand-written generics and trait objects.
- **One field-codec path — no `#[nested]` marker.** A field is either a `Bits` leaf (a
  `uN`/`#[bitfield]`/enum — a single packed value, read by reading its bits) or a nested
  message (another `#[bin]` type — a layout of fields, read by recursing into its codec).
  A proc-macro can't tell the two apart by type name, so the codec once needed an explicit
  `#[nested]` marker. Instead, **every `Bits` leaf also implements
  `BitDecode`/`BitEncode`/`FixedBitLen`** (thin delegations to its bit read/write), so
  `#[bin]` calls those uniformly for *every* field. These are **concrete** impls — one per
  leaf type, emitted by each `Bits`-producing macro — *not* an `impl<T: Bits>` blanket,
  which Rust's coherence rejects against the per-message derives (no specialization, no
  negative bounds). The `Bits` *packing* role is untouched; only the stream-codec impls
  were added. `#[nested]` is still accepted as a no-op for backward compatibility.
- **Position-aware errors.** A codec error records the absolute **bit offset** where it
  failed and the **field** being processed (the innermost wins, like a span), so a
  failure points at the exact place. A streaming source that runs out mid-message
  reports `Incomplete` ("read more and retry"), distinct from a definitive failure.
- **Reserved bits are explicit, stored, and observable.** A `#[reserved]` field is a
  normal stored field with a known *spec value* (the type's zero, or the
  `#[reserved_with(…)]` expression). On the verbatim path (`decode`/`to_bytes`) it
  reads/writes its actual value, so a peer's non-compliant reserved bits are captured and
  a caller can override them (dual-use); the builder defaults it to the spec value (so it
  isn't required), and the **canonical** encoder (`to_canonical_bytes`) writes the spec
  value instead. A *verified-on-read* constant is `magic` instead.

## 9. Performance

Bitfields are plain shift/mask on a single backing integer — fully monomorphized, no
`bitvec`, no per-field heap, no runtime field tables. Benchmarked against the crates it
collapses (`bitbybit`, `modular-bitfield`) and a hand-written shift/mask baseline on an
identical DNS-shaped 16-bit field, `bnb` matches `bitbybit`, beats `modular-bitfield`,
and is within noise of hand-written (pack ~870 ps, unpack ~190 ps). The stream codec
takes a byte-aligned fast path — when a read/write is byte-aligned (the common case for
headers and `[u8; N]` payloads) it copies whole bytes instead of shifting one bit at a
time (~2–3× on aligned data); sub-byte reads fall through to the general bit loop. The
generated accessors and the runtime read/write methods are `#[inline]` so they inline
across crate boundaries.
