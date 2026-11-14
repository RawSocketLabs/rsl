# `bnb` — design rationale

`bnb` is an owned, bit-aware binary codec: arbitrary-width integers, bitfields,
enum⇄integer mappings, flag sets, a required-by-default builder, and a unified
`#[bin]` whole-message codec — one crate, integer-backed, fast. This document is the
design rationale: the problem it solves, the shape of the solution, and the decisions
behind it. For runnable walkthroughs see the [`bnb::guide`] module; for credit see
[`ACKNOWLEDGMENTS.md`](ACKNOWLEDGMENTS.md).

[`bnb::guide`]: https://docs.rs/bitsandbytes/latest/bnb/guide/

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
  runtime — cursors, traits, the I/O ladder) are the core; `codecs`, `wirelen`, the
  feature-gated `net` and `framing`, and `guide` complete it (see `lib.rs` for the full
  list). Re-exports the macros, so users depend only on `bnb`.
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

### 3.4 Const accessors: dispatch around the trait (0.3.2)

The generated accessors are `const fn` (parity with `bitbybit`, which we replace).
A `const fn` cannot call trait methods on stable Rust, so accessor *values* cannot
go through `Bits::from_bits`/`into_bits` the way the *widths* go through
`Bits::BITS` (associated consts are fine in const-eval; method calls are not).
The accessors therefore dispatch by type: `bool` and the primitive unsigned
integers convert inline in the generated code, and every other field type is
called through a hidden inherent pair with the trait's exact contract —
`__bnb_from_bits(u128) -> Self` / `__bnb_into_bits(self) -> u128` — which `UInt`
and every `Bits`-producing macro emit, and to which the real `Bits` impls
delegate (single source of truth, so trait and const paths cannot drift). The
trait keeps working everywhere else (codec, derives); a hand-written `Bits`
field type gets the pair via `impl_bits!`, which emits the trait impl and the
inherent pair from one definition (so they can't disagree, and the pair's
naming stays an implementation detail). `#[view]` closures are inlined into the
accessors when their raw type is annotated, with a `dynamic` opt-out for
non-`const` bodies and a `const` argument that asserts const-ness (fallbacks
become errors).

**Watch: this dispatch is a stopgap, not the endgame.** Probed on rustc 1.97.1
(2026-07): `const trait` impls (RFC 3762) and closure calls in `const fn` are
both still unstable, so dispatching around the trait is the only design that
works on stable at *any* MSRV today. When `const trait` stabilizes, the plan is
to make `Bits` a `const trait`, delete the inherent-pair dispatch, and take the
MSRV jump — a coordinated breaking release (custom field types change from
`impl_bits!` to `impl const Bits`).

### 3.5 Layout: `repr(transparent)` (0.3.2)

The emitted struct wraps exactly one native integer, so it is emitted
`#[repr(transparent)]`: size, alignment, and ABI are the backing type's —
a claim `repr(Rust)` never made (before 0.3.2 the layout was formally
unspecified) and the honest version of the `repr(C)` that `bitbybit` emitted
(`transparent` is strictly stronger for a single-field struct, and what an FFI
consumer actually wants). A user-supplied `#[repr(...)]` suppresses the
generated one — `repr(C)`/`repr(align(N))` cannot combine with `transparent`.

## 4. Field types and macros

- **`u1`..`u127`** (`UInt<T, N>`) — range-checked sub-byte integers backed by the
  smallest sufficient primitive. Checked (`try_new`), panicking (`new`), and masking
  (`from_raw`) constructors.
- **`#[bitfield]`** — packs typed `Bits` fields into one backing integer, with
  `bits = msb|lsb` and `bytes = big|little` as independent knobs, and inferred /
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
one struct, generating the decode entry points (`decode`/`decode_all`/`decode_iter`/
`peek`/`decode_exact`), the encode entry points (`to_bytes`, plus `to_canonical_bytes` for a
message that has a `reserved`/`calc` field — see §5.2), the `encode(writer)` convenience
(and `BitEncode::bit_encode` for writing into a `Sink`), and construction
(a struct literal or `Type::builder()`). Fields are read and written at arbitrary bit
offsets, so the same attribute handles byte-aligned headers and sub-byte frames, and any
`Bits` type *or* nested `#[bin]` message drops in as a field — both decoded, encoded, and
sized through one uniform codec path, with no marker (§8).

**Struct-level options:** `big`/`little`, `bits = msb|lsb`, `magic = <expr>`
(a leading constant verified on read, emitted on write — any `Bits` value, so it can be
sub-byte), `read_only`/`write_only`, `no_builder`, `forward_only`, `ctx(name: Ty, …)`,
and `validate = <path>`.

**Field directives** (`#[br]`/`#[bw]`, the inherited vocabulary): `count`, `ctx { … }`,
`temp` + `calc`, `if(…)`, `map`/`try_map` (+ inverse `bw(map)`),
`parse_with`/`write_with`, `ignore`, `pad_*`/`align_*`, `restore_position`, and
`#[reserved]`/`#[reserved_with(…)]`.

`#[bin]` emits codecs directly through shared generators; the bare
`#[derive(BitDecode, BitEncode, BitsBuilder)]` derives
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
tagged-union enum encodes verbatim (no `to_canonical_bytes`/`validate`).
Those are properties of a concrete record; an enum's per-variant payloads define them, not
the union.

### 5.2 Encode model, construction, and validity

A message has two **encode forms**. `to_bytes` is **verbatim** — exactly what's stored, so
`decode → to_bytes` round-trips byte-for-byte and a deliberately-wrong field goes on the
wire as-is (dual-use). `to_canonical_bytes` is **canonical** — `reserved` fields written as
their spec value and `calc` fields recomputed, spec-compliant for the message's own fields.
Canonicalization is **per message, not deep**: a nested message or `Vec` element is written as
stored, and the parent's `is_canonical`/`canonical_diff` do not inspect it (the macro cannot
tell which field types are messages, and there is no in-memory canonicalization trait to
forward to); canonicalize such a child first. Deep canonicalization is an open item for the
1.0 API review ([`ROADMAP.md`](ROADMAP.md) §C). The two differ only
when a message has a `reserved` or non-`temp` `calc` field (a `temp`+`calc` field is never
stored, so it always recomputes and creates no gap), so `to_canonical_bytes` and the
in-memory helpers `to_canonical`/`canonical_diff`/`is_canonical` are generated only then.

The verbatim/canonical choice is made **per call**, not carried on the value. The two `Vec`
encoders are explicit (`to_bytes` vs `to_canonical_bytes`), and over a writer the `std`-writer
`encode(w)` is **always verbatim** (== `to_bytes`); to stream the canonical form, normalize
first — `value.to_canonical().encode(&mut w)`. There is no hidden mode field, so a
`reserved`/`calc` message is an **ordinary struct**: it is constructed via a struct literal,
the builder, or `decode`, and it **coexists with `serde` derives** (see §5 / the freeze note in
[`ROADMAP.md`](ROADMAP.md)).

`validate = path` (the construction-soundness check `build()` runs) is also exposed as
re-runnable `validate()` / `is_valid()` methods: `build()` checks once, but a value can be
mutated before sending, so these re-check the *current* value (computed, never a stored
flag). By convention `validate` expresses **semantic** soundness — not the representational
`calc`/`reserved` fields — so validity holds for the canonical form too; `to_canonical_bytes`
stays a pure normalization (compose `validate()` before sending if you want the check).

## 6. The I/O ladder

For a buffer already in hand, the slice entry points decode in the message's *own* byte/bit
order: `decode_exact` (one message, reject trailing bytes), `decode_all`/`decode_iter` (every
message in a `&[u8]`), and `peek` (no consume). For a cursor — a stream, a socket, a growable
buffer — `decode(&mut Source)` is the single cursor decode (and `BitEncode::bit_encode` writes
into any `Sink`):

| Source | Backing | Seek | Use |
|---|---|---|---|
| `BitReader` | `&[u8]` | free (cursor math) | in-memory bytes |
| `StreamBitReader` | any `Read` | no (forward only) | a stream read once |
| `BufSource` | any `Read` | yes (bounded retain-and-seek) | a socket that also seeks |
| `BitBuf` | owned `Vec<u8>` (pushable) | yes (cursor math) | incremental framing: push bytes, pull messages |
| `SeekReader` | `Read + Seek` | yes (via `io::Seek`, before every read) | a large file/container, cursor may be shared |
| `BufSeekReader` | owned `Read + Seek` | yes (tracked; relative seek only on a jump) | a large file/container you own (§14) |
| `BytesReader`/`Writer` (`bytes` feature) | owned `Bytes` | yes | zero-copy async framing |

A `Source` carries its own byte/bit order, so `decode(&mut Source)` reads in the *cursor's*
order — correct for the msb/big default, but a `little`/`lsb` message wants a cursor built with
its layout (`BitReader::with_layout(&v, Msg::LAYOUT)`). The slice entry points sidestep that by
baking `Msg::LAYOUT` in, so they are the foolproof "decode this buffer" path; `decode(&mut Source)`
is for streaming and custom sources.

**Byte order × bit order — the natural-layout rule.** The two knobs compose by one rule:
each bit order has a *natural* byte layout — the bytes the bit cursor produces with no
transform. MSB-first emits a value's high bits first, so its bytes land **big-endian**;
LSB-first emits low bits first, so its bytes land **little-endian** (value bit *k* goes to
stream bit *k*: exactly the CAN/DBC "Intel" layout, `raw |= v << start; frame =
raw.to_le_bytes()`). The declared byte order swaps a **byte-multiple** value only when it
*differs* from that natural layout; sub-byte widths are never byte-swapped. So the two
identity corners are the two real-world conventions — `big`+`msb` is network order,
`little`+`lsb` is DBC-Intel/SMB — and the mixed corners (`little`+`msb`, `big`+`lsb`) are
the deliberate swaps. Pinned against the embedded DBC reference formula in
`tests/bin_lsb_dbc.rs` (golden + property-tested), and golden at every layer in
`bin_order_matrix.rs` (message) and `bitstream.rs::cursor_layout_matrix` (cursor). The
`#[bitfield]` layer agrees by construction: `bits = lsb` packs `v << offset` into the
backing integer and `bytes = little` emits `to_le_bytes` — the same DBC layout.

Seeking is only needed by a message that uses `restore_position`; everything else runs
over the forward-only `StreamBitReader` too. **Seeking is a source capability, enforced
in the type system:** when a message uses `restore_position`, the generated
`decode` is bound on `SeekSource`, so decoding it through a forward-only stream is
a compile error rather than a runtime surprise; `forward_only` is the opt-in that
forbids seek directives outright. The in-memory cursor needs no `Seek` trait at all —
the whole buffer is in hand, so a seek is just cursor arithmetic (which also enables
e.g. DNS name-compression pointer following).

Above the cursor ladder, the opt-in **`net`** feature adds whole-message helpers over `std`
sockets — `MessageStream` (`read_message`/`write_message` over any `Read + Write`: one value drives
both directions of a `TcpStream`, no `try_clone`) and `MessageDatagram` (`send_message`/
`recv_message` over a sealed `DatagramSocket` — see §8). Both decode in the message's own layout,
and `MessageStream` reuses `BitBuf`'s framing rather than re-rolling it. The **`mock`** feature adds
in-memory `MockDatagramSocket`/`MockStream` (scripted inbound, captured outbound, chunked delivery,
error injection) so `net` code is unit-testable without a real socket.

### 6.1 `no_std` and the `std` feature (Option A)

`bnb` is `no_std` + `alloc`. `alloc` is unconditional — the codec's output model *is*
`Vec<u8>` (and `count` payloads / error strings own heap), so a heapless variant would
be a different crate, not a feature. The default-on **`std`** feature adds only the
rows of the table above that are backed by `std::io` (`StreamBitReader`/`BufSource`/
`SeekReader`, the `as_read`/`as_write` views), the `From<std::io::Error>` bridge +
`ErrorKind::Io`, and the `encode(writer)` convenience. The
forward-only/seekable distinction is unchanged; `no_std` simply has fewer `Source`
implementations to feed `decode` (the in-memory `BitReader` and `BitBuf`, and `BytesReader`
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
  default `cargo add bnb`. `encode(w)` is **unconditionally verbatim** (== `to_bytes`, dispatching
  to `bit_encode`); the form is chosen **per call**, not carried on the value, so streaming the
  canonical form is `value.to_canonical().encode(&mut w)`. `canonical_bit_encode` remains a
  **defaulted method on `BitEncode`** (no separate `CanonicalEncode` trait), overridden by the
  derive only for a `reserved`/`calc` message. Cost: callers bring the trait into scope
  (`use bnb::prelude::*`); the `to_bytes`/`to_canonical_bytes` `Vec` encoders stay inherent and
  unconditional (sink-writing uses the `BitEncode` trait methods).
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
- **One cursor decode.** Decoding funnels through a single `decode(&mut Source)` over the I/O
  ladder, plus the layout-baking slice helpers (`decode_exact`/`decode_all`/`decode_iter`/`peek`).
  An earlier byte-cursor `decode(&mut &[u8])` was removed: a `&[u8]` cursor advances only whole
  bytes, so it silently misframed messages that don't end on a byte boundary — the one thing a
  *bit* codec must get right. `BitReader::new(&v)` is *also* zero-copy, so
  `decode(&mut BitReader::new(&v))` replaces it at no cost with a bit-correct cursor. The price is
  that a cursor carries its own byte/bit order (§6); the slice helpers bake the message's, keeping
  the foolproof "decode this buffer" path foolproof.
- **Position-aware errors.** A codec error records the absolute **bit offset** where it
  failed and the **field** being processed (the innermost wins, like a span), so a
  failure points at the exact place. A streaming source that runs out mid-message
  reports `Incomplete`, distinct from a definitive failure. Safe retry requires retained
  input (`BitBuf`); the signal alone does not make forward-only reads transactional.
- **Reserved bits are explicit, stored, and observable.** A `#[reserved]` field is a
  normal stored field with a known *spec value* (the type's zero, or the
  `#[reserved_with(…)]` expression). On the verbatim path (`decode`/`to_bytes`) it
  reads/writes its actual value, so a peer's non-compliant reserved bits are captured and
  a caller can override them (dual-use); the builder defaults it to the spec value (so it
  isn't required), and the **canonical** encoder (`to_canonical_bytes`) writes the spec
  value instead. A *verified-on-read* constant is `magic` instead.
- **Sealed extension trait where the genericity is internal.** `DatagramSocket` (the `net`
  feature) exists only to make `MessageDatagram` generic over `UdpSocket` and `UnixDatagram` — std
  ships no datagram analog of `Read + Write` — not as an open extension point. So it is *sealed* (a
  private `Sealed` supertrait): downstream can still *use* it as a bound (a generic handler) but
  can't *implement* it. That keeps the 1.0 promise to two methods rather than a frozen,
  openly-implementable trait, and leaves bnb free to evolve it — or to ship the feature-gated
  `MockDatagramSocket` itself. Testing was the one real reason to implement it downstream, and the
  `mock` feature covers that; a trybuild test locks the seal in. (`MessageStream` needs no such
  trait — it's generic over std's `Read + Write`, so a `std::io::Cursor` or `MockStream` already
  mocks it.)

## 9. Builder enum-alias normalization

SOCKS method offers exposed a mismatch between decoding `0xff` as `NoAcceptable` and
constructing `Other(0xff)` with the same wire code. Builders now normalize these aliases
automatically after resolving all required/default fields and before semantic validation.
`NormalizeEnumAliases` is an additive public trait: `BitEnum` derives reuse their existing
integer mappings; ordinary `#[bin]` structs/enums and standalone `BitsBuilder` structs
traverse supported fields. `Vec`, `Option`, and arrays recurse without allocating or changing
shape. Packed bitfields already reconstruct enums from their integer storage.

Concrete-site autoref dispatch selects the trait implementation or an opaque fallback without
adding bounds to arbitrary existing fields. It works on MSRV 1.85; placing the dispatch inside
a generic helper would select the fallback too early. This does not expand generic macro support.

Custom mapped/codec newtypes remain opaque unless their author implements the trait. Field-level
mapping/custom-codec directives and `ignore`/`temp`/read or write `calc` fields are excluded.
A wrapper used as an ordinary field is the explicit extension seam. Unknown discriminants,
reserved deviations, counts, and collection order survive. Raw construction, mutation, immutable
validation, decode, and both encode forms are unchanged; validators still need defensive checks.

Compatibility: this changes Rust value equality on the builder path (also for plain builders).
The guarantee is discriminant identity, not byte identity under arbitrary callbacks: calculations
or mappings referring to normalized siblings can distinguish variants. Ordinary codecs retain
the same bytes. There is an extra collection traversal before validation, including oversized
collections that a validator will subsequently reject. No opt-out or general repair framework
is added. The next smallest protocol slice remains SOCKS's explicit raw/malformed surface.

Verification (offline, 2026-09-06): nine focused macro tests, the SOCKS suite (35 tests plus
one doctest), workspace tests, and the `bytes`/`mock`/`tokio` feature suites pass. All four
runtime normalization mutants were caught. The extended decode fuzz target completed two
million cases without failure. Workspace MSRV 1.85 checking and the renamed-dependency smoke
crate's MSRV check pass; the latter also builds for `thumbv7em-none-eabi` on stable. Formatting
and workspace Clippy pass with existing warnings; the new focused test target adds none.
The pinned `nightly-2026-06-17` public-API output matches the updated snapshot (additions only).

Subsequent 0.4 release preparation resolves the original bnb verification debt: the
33 macro and 84 runtime Clippy diagnostics, plus all-target example/test warnings,
are fixed or narrowly justified where intentional (const conversions, byte extraction,
generated comparison types, and CLI output). Strict all-target/all-feature Clippy
passes for both crates without crate-wide suppression. Warning-free all-feature
rustdocs pass. `wnaf 0.14.1` replaces the yanked 0.14.0; cargo-deny passes all four
checks without an advisory exemption. Syn is updated to 3.0.5 and benchmark-only
bitbybit to 2.0.1. No new production dependency or MSRV increase is introduced.

`cargo-semver-checks 0.50.0` passes against published 0.3.2 with all, default, and no
default features; the public API snapshot remains unchanged by release cleanup.
Cargo verifies both distributable archives using its temporary registry, including
the matching unpublished macro crate. The two-million-input fuzz smoke and four
caught runtime normalization mutations pass again. Runtime/all-feature and renamed
no_std consumer checks pass on 1.85; developer tests/benchmarks intentionally use
stable (`criterion 0.8.2` needs 1.86, `trybuild 1.0.120` needs 1.88).

Release versions remain automation-owned. The intended 0.4 builder behavior change
needs a breaking commit marker despite the additive source API; see
[`RELEASING.md`](../docs/RELEASING.md) for ordering, credential preflight, checks, and
remaining hosted verification requirements. No release side effects are authorized
by local preparation.

Independent final review approved after unifying bare-derive field exclusions and confining
autoref helpers to the explicitly unstable `__private` export path; no findings remain.

### 9.1 Consuming alias normalization

`NormalizeEnumAliases::into_normalized_enum_aliases(self) -> Self` delegates to
the existing in-place operation and returns the moved value. Only this default
method requires `Self: Sized`; the trait remains dyn-compatible and imposes no
`Clone` or `Copy` bound. Copy enums support expression-style comparisons;
non-Copy messages and collections move, with an explicit caller-side clone when
the original must survive. No borrowed `as_` view or implicit-clone helper is added.

The helper does not allocate, validate, repair lengths/reserved fields, or expand
traversal through opaque fields. Generated builders, macros, and wire behavior
are unchanged. This shipped as a runtime-only additive release, `0.4.1` with
macros remaining `0.4.0`. CI's source baseline advances to published `0.4.0` in
all/default/no-default feature modes with `--release-type patch`; release-plz still
selects actual versions.

Local verification (2026-09-07): eleven focused macro tests, seven builder unit
tests, doctests, workspace and bytes/mock/all-feature suites pass. Removing the
delegation makes the non-Clone unit test fail; restoring it passes. Strict bnb/macros
Clippy, configured workspace Clippy, denied-warning all-feature/no-default docs,
Rust 1.85 workspace/all-feature/renamed-consumer checks, and bare-metal compilation
pass. The pinned API snapshot is additive; all three 0.4.0 compatibility checks,
cargo-deny, benchmark smoke, and two million fuzz inputs pass. All fifteen container
validation jobs, hosted CI, and the generated release candidate passed; PRs #73/#74
were merged and the runtime published under `michael-smythe`. Independent review found no
remaining issues after tightening compatibility checks to the patch release class.
The next smallest consumer change is SOCKS immutable method validation using the
consuming helper on copied methods; no protocol surface expansion is needed.

## 10. Performance

Bitfields are plain shift/mask on a single backing integer — fully monomorphized, no
`bitvec`, no per-field heap, no runtime field tables. Benchmarked against the crates it
collapses (`bitbybit`, `modular-bitfield`) and a hand-written shift/mask baseline on an
identical DNS-shaped 16-bit field, `bnb` matches `bitbybit`, beats `modular-bitfield`,
and is within about 4% of hand-written (pack ~873 ps against ~843 ps, unpack ~428 ps
against ~434 ps; the earlier "unpack ~190 ps" was the benchmark harness folding its input,
§19). The stream codec works per touched byte, never per bit. A byte-aligned read or write
(the common case for headers and `[u8; N]` payloads) concatenates whole bytes, and a read of
at most eight bits is taken from the one or two bytes it touches. On the slice reader those
two paths and the bounds check are forced inline into generated decoders, where a field's
constant width folds them down, and a typed read of whole bytes from a byte-aligned cursor
is one load and at most a byte swap (§22, phase 2, which is also where `read` itself, the
call a generated decoder makes, became inline). Writing mirrors it: `write` is inline on
the writers that append, and a whole-byte value at a byte-aligned cursor is one append
(§22, phase 3). A wider read that starts or ends inside a byte gathers the
bytes it touches into one `u128` window and shifts the field out; an unaligned write ORs
into each touched byte the bits that fall there. Both live in functions kept out of line so
the inlined paths stay small (§19 has the measurements behind these choices). Byte runs
(`Vec<u8>`, `[u8; N]`) go further: a byte-aligned run is one slice copy (§16). The generated
accessors are `#[inline]` so they inline across crate boundaries.

## 11. Incremental decoding and lossless handoff (0.5.0)

### 11.1 Scope and existing capabilities

This extends **existing** `BitBuf`, `MessageStream`, `BinCodec`, `DecodeWith`, and
`ErrorKind::Incomplete`; it does not add a second framing engine, parser generator,
transport runtime, or dependency. `pull` already decoded zero/one/many messages through
repeated calls, but hid shortfall details; socket extraction and bounded read-ahead could
lose retained input. Direct `Source` decoding, `BufSource`, and `StreamBitReader` are not
transactional substitutes. Inspection covered runtime cursors/I/O/codecs/integers/fields,
macro generation/dispatch/builders, feature boundaries, existing tests/benchmarks/fuzz,
public API, and release gates. Findings below include pre-existing shared-path weaknesses,
not just lines introduced by this feature. This is a bounded audit, not a proof of the
absence of defects in arbitrary downstream codecs.

Baseline: `25a4381ae353421848a9bb61d9260623029829cc`. Candidate work is isolated on
`feat/bnb-incremental`; the existing SOCKS integration worktree/index is untouched.
No SOCKS source is changed in this slice. Local implementation evidence is separate from
delivery: repository/advisory metadata was refreshed on 2026-09-08; hosted candidate CI,
merge, generated release versions, and registry publication are subsequent gates.

### 11.2 Contracts and migration

| Surface | Decision |
|---|---|
| `BitBuf::pull` | Retained `Result<Option<T>, BitError>` convenience. |
| `try_pull` | One complete value or typed `Incomplete` with bit offset, field, and optional **additional-byte** hint. Drain until incomplete. |
| `pull_eof` | One finite attempt; empty is `None`, truncation is a hard error. EOF is not sticky. |
| `try_pull_with` / `pull_eof_with` | Explicit layout and `DecodeWith<A>` arguments; supports read-only codecs, fresh non-`Clone` context each attempt. |
| Failure | All attempt errors preserve buffered bytes and cursor. No callback-side-effect rollback or parser continuation state. |
| `push` | Single fallible capacity-enforcing append; replace old `try_push` with `push`, handle its result. Rejected input stays with caller; no pre-rejection compaction. |
| `MessageStream` extraction | Replace lossy `into_inner` with `try_into_inner`, or transfer `(stream, BitBuf)` via `into_parts` / `from_parts`. |
| Raw stream I/O | `Read` returns buffered bytes first without also reading the stream; unaligned imported cursor errors unchanged. `Write`/`flush` delegate. |
| Message boundaries | Core `BitBuf` preserves exact bits; sync and Tokio helpers consume each message's final byte padding, matching independent encoding. |
| Tokio handoff | Transfer both `FramedParts` buffers. `BinCodec::decode_eof` is finite; errors retain a typed `BitError` inside `io::Error`. |

Only the private incremental backing reader converts **physical** shortage to `Incomplete`.
Logical `LimitedSource` exhaustion and custom hard errors remain definitive. An explicit
custom `Incomplete` at EOF becomes `IncompleteAtEof`, retaining its hint and location without
inventing an exact missing-bit count. A successful streamed message or counted element must
advance; otherwise `NoProgress` prevents an infinite drain/allocation loop. Zero-width values
remain usable outside these progress-requiring operations.

Variable-length magic dispatch preserves declaration order: a matching but incomplete prefix
waits before a later variant/fallback. Finite input may fall back; a definite mismatch may
fall back immediately. Probing rewinds on failure and allocates no scratch byte vector.

`BitDecode::decode_vec` is a defaulted extension point with an exact-count/wire-order/progress
contract. The primitive `u8` override uses `Source::read_bytes`; generated context-free counted
fields use this method rather than guessing types by spelling. The incremental source proves
whole-payload availability before allocation. Custom/context element codecs retain their
semantics; wrappers using the default byte reader can still allocate incrementally.

This shipped as a **0.5.0 pair**: breaking runtime changes and generated calls to new runtime
helpers must not ship as macros compatible with `^0.4`. Release-plz generated both versions
and the root dependency pin from the breaking Conventional Commit. The reviewed API deltas
record all, default, and no-default feature surfaces against published 0.4.0, including the
already-existing 0.4.1 consuming alias helper. Major-mode semver checking was paired with
those exact deltas, not used as a blanket waiver. Post-release CI enforces patch compatibility
against published 0.5.0; `api-delta-0.5-{all,default,none}.txt` remain historical migration
records, not executable gates. The one-time delta checker is retained only in Git history.

### 11.3 Audit findings and disposition

All entries below are pre-existing unless marked candidate. High findings block the candidate
until fixed and proven; accepted medium/low follow-ups do not affect the stated incremental
contract. Tests are named by behavior; source locations are module/function names so this
ledger survives line movement.

| Severity / location | Consequence and evidence | Disposition / closing proof |
|---|---|---|
| High: `net::MessageStream` extraction/read loop | `into_inner` discarded tails; reading before a capacity check could lose bytes. | Checked/parts extraction, bounded reads before I/O; component tests recover every byte across capacity errors, consumed prefixes, phase changes, and raw handoff. |
| High: `bitstream::CountPrefix::to_count` | Wide untrusted counts narrowed by wrapping. | Saturate above `usize::MAX`; host-width boundary and hostile counted-field tests. No eager allocation from count. |
| High: explicit `#[br(count = expr)]` (final reviewer) | Both generated paths still cast to `usize`; a wide count could wrap to zero and falsely complete a frame. | Evaluate once with checked `TryInto<usize>` before payload decode; negative/overflow counts are field-positioned conversion errors. Direct u128, context, ordinary scalar/literal, and unchanged-buffer regressions. |
| High: generated counted collections | A zero-width successful element allowed count-driven work/allocation without input progress. | Default `decode_vec` and context loop progress guards; runtime, generated, and context regressions. |
| High: `bitenum` discriminant/width generation | Auto-increment overflow, out-of-width values, and misleading width aliases could create lossy or non-total mappings. | Checked increment and compiler-evaluated actual-width/discriminant/exhaustiveness assertions; three new compile-fail cases. |
| High: `bitfield` explicit ranges | A range beyond the backing integer produced invalid layout arithmetic. | Reject at macro expansion with field span; compile-fail test. |
| High: `int::UInt<T, N>` | A width beyond the backing primitive misrepresented its domain. | Const-evaluated width invariant on public constructors/constants/trait width; compile-fail test. |
| High: `StreamBitReader::read_bits` | Non-EOF I/O failures were reclassified as retryable incomplete input. | Preserve `Io(kind)`; scripted I/O test and explicit nontransactional docs. |
| High: `MessageDatagram::recv_message` | Trailing full bytes in the received slice were silently accepted/discarded. | Exact received-slice decode; extra-byte rejection and final-padding acceptance tests. An undersized OS receive buffer still truncates, as documented. |
| High: `BitBuf::grow` / bounded clone | Sparse grow/clone did not reserve the promised full logical cap, allowing later reallocations. | Reserve relative to length; manual bounded clone reservation; allocator profile proves later fills allocate zero. Unbounded clone copies length, not spare capacity. |
| High: `BitBuf::make_room` after grow | Allocator slack could permit retained physical length beyond the logical cap; rewinding made all bytes live and capacity subtraction could panic. | Compact against the logical cap when bounded, physical capacity otherwise. Public regressions cover byte/partial-bit cursors, unchanged overflow rejection, rewind, and exact data both after grow and at the exact original cap. The no-grow regression kills the surviving capacity-arithmetic mutation. |
| Medium: `BufSource` / `SeekReader` positions | Cursor plus requested width could overflow. | Checked addition before I/O; host-limit tests assert typed error and unchanged cursor. |
| High: explicit `#[br(seek = expr)]` (final reviewer) | A wide wire pointer narrowed to a valid earlier offset. | Checked `TryInto<usize>` with field-positioned conversion error; wide/signed offsets, valid pointer, and unchanged-buffer regressions. |
| Medium: `BitAmount::bytes` | Debug-only overflow panic diverged from release wrapping. | Explicit low-32-bit wrapping, documented unchecked conversion, boundary tests. |
| Medium: byte-order transform | A downstream invalid `Bits::BITS > 128` could panic before width validation. | Let the sink report `TooWide`; custom-width regression and documented trait domain. |
| High: candidate magic/EOF/error bridges | Broad EOF conversion or trying fallback too early would hide malformed input or consume the wrong variant. | Private physical-shortage adapter, finite retry, typed `as_read` bridge; split-prefix, logical-boundary, custom-error, and EOF tests. |
| Medium: explicit field width vs logical/raw type | A wider stored field than its logical type can hide high bits through getters (raw backing still retains them). | Deferred policy decision: reject versus explicitly permit truncating views. Not a new incremental path; track before 1.0. |
| Low: explicit bitflag aliases | Duplicate positions are accepted but alias/iteration policy is not explicit. | Deferred API/documentation decision; no changed flag generation. |
| Medium: `auto_len = bytes` | Probe uses a default-layout, offset-zero, scratchless writer and invokes the encoder twice. Stateful lengths can differ. | Document supported state-independent/retry-safe encoder boundary; use encode-once outer envelopes for such protocols. Track general encode-once sizing separately. |
| Low: `SeekReader` allocation / macro offsets | At most 17 temporary bytes are heap-allocated per scalar; cumulative generated offset expressions can grow quadratically. | Track stack scratch and measured codegen optimization independently; neither is required by this buffer-backed design. |
| Low: fixed-array width constants | Extremely large array-size expressions can exceed the `u32` fixed-bit-length domain. | Track clearer compile-time diagnostics; runtime length paths here are checked. |

### 11.4 Genericity and limits

Evidence is not SOCKS-specific: existing DMR golden vectors at every split, 54-bit packed
frames in both bit orders, compressed DNS in an owned TCP-length envelope at every split,
versioned read-only records with context and explicit layout, nested logical bounds,
variable magic/fallback dispatch, arbitrary partition TLV properties, and a borrowed DER
parser over the extracted owned envelope. DNS compression pointers remain relative to
the message, not an accumulating stream buffer. The DER test proves the borrowed span
points into the owned envelope; it adds no owned trait requirement to the DER parser.

Hints describe the next blocked operation, not the full frame size or a no-read-ahead
promise. Error positions are buffer-local and can rebase after compaction. Buffer caps
bound retained **wire bytes**, not allocations in returned values, parser recursion, or
arbitrary callbacks. **Recursion is depth-limited** (§20, 0.10.1): a message type can contain
itself only through a counted `Vec`, and decoding fails with `ErrorKind::TooDeep` past
`MAX_DECODE_DEPTH` nested non-empty `Vec`s of messages (the abort reproduced in the 0.9.0
review). The limit counts levels, not stack bytes: a message with a large decoder frame can
still overflow a small thread stack below it, so for such types the caller sizes the stack
for 128 frames or caps the frame size. Recursion inside a `parse_with` codec or a
hand-written `BitDecode` impl stays the codec's to bound. General variable-element parsing replays and can be quadratic under
tiny fragments: batch input, cap frame sizes/attempt frequency, or frame an opaque owned
envelope first. No generalized resumable parser is implied. Async cancellation/partial
writes remain owned by Tokio/transport callers; this does not make whole-message retries
safe after a partial write. Direct underlying reads can still bypass buffered tails.
`IncompleteAtEof` classifies custom shortages in the new finite-attempt APIs only. Legacy
finite custom-codec entry points (including datagram `decode_exact`) must themselves return
definitive errors; broader finite-error normalization is deferred. A datagram already
truncated by an undersized receive buffer cannot be reconstructed by exact slice decoding.

Verbatim encoding retains modeled wire fields, not arbitrary unmodeled padding or a custom
codec's original representation. `decode_exact` accepts final partial-byte padding and an
independent encoder emits zero padding. Bit-exact packed concatenation uses one bit cursor.

### 11.5 Performance gate and measurements

Setup: rustc 1.98.0 / LLVM 22.1.8, default features, system allocator, Threadripper 7970X,
CPU 0 pinned (`taskset`, SMT sibling 32), powersave governor, boost enabled. No concurrent
builds during timed runs. Baseline source is the SHA above plus the identical benchmark
harness. Criterion uses 100 samples/3 s warmup/5 s measurement for IPv4-shaped messages;
u32-length byte envelopes of 64/1024/16384 bytes, chunks 1/64/1500/whole, use 10 samples,
200 ms warmup/1 s measurement (extended for slow cases). Baselines saved as
`bnb-04-final-a` / `-b`; final candidate as `bnb-05-release-a` / `-b`.

**Predeclared gates**, established before candidate measurement: baseline repeat variation
under 5%; investigate/reject unexplained >15% whole-message/envelope regression (3× noise).
Incomplete byte payloads allocate zero, completion allocates one payload. The 16 KiB/1-byte
case must scale by at most 20× the 1 KiB case (16× input plus 25% tolerance). Separately
investigate >15% expanded-source growth for unchanged `bin_message`; repeated warm expansion
times are diagnostic, not cold-build claims. These are controlled local gates, not noisy
shared-CI timing assertions. General variable-element replay is measured without claiming
linearity or extrapolating the byte-blob optimization to arbitrary codecs.

Final A/B central estimates (both candidate runs follow the last production capacity fix):

| Case | Baseline A / B | Candidate A / B |
|---|---|---|
| IPv4-shaped encode | 239.74 / 241.86 ns | 255.19 / 238.45 ns |
| IPv4-shaped decode | 244.92 / 236.92 ns | 132.08 / 128.72 ns |
| Whole 16 KiB byte envelope | 38.856 / 38.751 µs | 371.30 / 370.37 ns |
| 1 KiB envelope, one-byte chunks | 1.3681 / 1.4075 ms | 10.830 / 10.852 µs |
| 16 KiB envelope, one-byte chunks | 320.50 / 320.51 ms | 170.25 / 177.20 µs |
| 1024 variable elements, whole input | 23.672 / 23.925 µs | 27.176 / 26.140 µs |
| 64 variable elements, one-byte chunks | 96.521 / 92.357 µs | 96.916 / 93.995 µs |
| 1024 variable elements, one-byte chunks | 25.175 / 24.649 ms | 27.946 / 27.648 ms |

Baseline repeats differ by at most 4.91% relative to their pair mean across all 18 cases.
The candidate byte-envelope scaling is 15.72× / 16.33×, within the 20× gate. No tested
whole-message/envelope case exceeds the 15% regression budget even comparing the slower
candidate with the faster baseline. Generic collection whole-input decoding costs up to
14.8% more, accepted for per-element progress enforcement; it is not the optimized byte-blob
path. Its one-byte replay grows about 288× / 294× for 16× more elements: the quadratic
limitation is real, measured, and requires batching or opaque envelopes for such workloads.
IPv4 encode repeat variance is visible, so no encoding speedup is claimed. The unchanged
`bin_message` expansion grows from 55,343 to 57,800 bytes (+4.44%); repeated warm expansion
was 0.15 s baseline / 0.13 s candidate, diagnostic only. No cold-build speed claim is made.

Separate allocation instrumentation: debug `bitbuf_bounded --profile` under
`scripts/profile-allocations.gdb` on Linux x86-64, no unsafe allocator implementation and
no debugger network downloads. Passed: 1000 bounded scalar cycles allocate zero; sparse grow
and bounded-clone fills allocate zero; bounded clone reserves 64 bytes; unbounded clone of
length 3/reservation 1 MB allocates only 3 bytes. All 4095 partial attempts before completing
a 4096-byte payload allocate zero; completion allocates once (4096 bytes). Compaction plus
append moves four bytes (two retained + two new). Debug `memmove` counts also include compiler
aggregate/error copies; those totals are not a payload-copy metric and are excluded from
throughput comparisons. The spare-capacity phase (cap 65, append 64, consume 8 bytes,
append 1) allocates zero and moves only the newly appended byte, not the 56 retained bytes.
These copy counts are characterized on the stated debug toolchain and must be recalibrated
for different compiler lowering, not relaxed to accept unnecessary compaction. Script
assertions fail on allocation/copy-budget regressions.

### 11.6 Verification and review gate

The reusable pre-commit audit/performance/reviewer process is codified in
[`../docs/RELEASING.md`](../docs/RELEASING.md). Every finding has a disposition; no commit
may precede final independent review and resolution of affected verification failures.
Local checks on 2026-09-08 use separate candidate and immutable-baseline target directories.
Sharing one Cargo target between worktrees produced stale/cross-contaminated artifacts;
the reported baseline builds, expansion, and timings were rerun in isolated directories.
Rustdoc/public-API jobs also run serially when sharing a target, so a documentation build
cannot overwrite the JSON input of an API check.

| Gate / command | Result and interpretation |
|---|---|
| `cargo fmt --all --check`; `git diff --check` | Pass. |
| `cargo clippy -p bitsandbytes -p bitsandbytes-macros --all-targets --all-features -- -D warnings` | Pass without warnings. Historical macros warnings are resolved, not waived. |
| `cargo test --workspace`; `cargo test -p bitsandbytes` with default, `bytes`, `mock`, `tokio`, and all features | Pass, including macro/UI, properties, and transport tests. |
| `cargo test -p bitsandbytes --no-default-features --test incremental --test bitstream_dmr_frame` | Pass. The broad no-default **test** command still includes pre-existing ungated std-only examples/tests; reproduced on isolated main. This does not affect no_std library/renamed-consumer gates. |
| `cargo clippy --workspace --all-targets`; strict all-feature SOCKS Clippy | Pass under configured CI policy; workspace has the same 106 warning lines as baseline. Workspace-wide `-D warnings` still fails unrelated lint debt; no blanket waiver added. |
| `cargo +1.85.0 check --workspace`; runtime all features; renamed consumer | Pass. Developer tests/benchmarks use stable; library MSRV remains 1.85. |
| `cargo build --manifest-path codecs/bitsandbytes/bnb/nostd-check/Cargo.toml --target thumbv7em-none-eabi` | Pass on stable. The 1.85 bare-metal target is not installed locally; CI requires the checked 1.85 host plus stable bare-metal combination. |
| `RUSTDOCFLAGS='-D warnings' cargo doc` for both crates/all features and runtime/no defaults, `--no-deps` | Pass. |
| Pinned `cargo +nightly-2026-06-17 public-api` snapshot; historical `check-api-delta.sh` in `all`, `default`, `none` modes | Pass; exact reviewed delta guarded the 0.4 → 0.5 migration. The checker was retired after publication. |
| `cargo semver-checks -p bitsandbytes --baseline-version 0.4.0 --release-type major` in all/default/explicit-none modes | Pass, but major mode executes **zero** compatibility rules (254 skipped); it is not compatibility proof. The diagnostic patch run executes 223 rules: 222 pass, one fails for the intentionally removed `try_push`/`into_inner`. Both crates require 0.5.0. |
| `cargo package -p bitsandbytes-macros -p bitsandbytes --all-features --allow-dirty` | Both archives build together using Cargo's temporary registry. Current development manifest versions are not the release candidate versions; repeat on the generated release PR. |
| `cargo deny check`; `actionlint` | Pass with refreshed advisory data. Unused-license and duplicate-syn warnings remain; yanked wnaf is absent. |

Benchmark, allocation, mutation, fuzz, and final reviewer evidence below complement these
checks. A clean committed worktree must additionally pass `scripts/ci-act.sh pre-push` (since
replaced by `scripts/ci-local.sh`),
then exact-SHA hosted PR/main CI and release-PR/package checks before registry publication.

Final independent reviewer approval: 2026-09-08, no findings or unresolved pre-commit
release blockers. Approval covers the final production diff, exact-capacity regression,
allocation profile, findings dispositions, and reconciled measurements. Delivery gates
above remain mandatory; source changes require affected checks and review again.

The first containerized pre-push run exposed a new UI-environment mismatch, not a relaxed
baseline exception: the UInt width diagnostic includes core source excerpts locally, but
the container omitted them without `rust-src`. The build/test job now installs that
component, matching trybuild's documented prerequisite; the invalid-width assertion and
snapshot are unchanged. This delivery-only fix requires the full pre-push rerun.

Focused mutation evidence on the final production implementation:

- Core attempts/capacity/EOF/dispatch/extraction: 72 mutants, **62 caught, 6 unviable,
  4 functional survivors**. The exact-capacity regression added after the previous run
  catches `limit - len` changed to `limit / len`; it is not an accepted survivor.
- The four remaining `make_room` variants retain mandatory compaction and only change
  its discretionary schedule: computing live bytes with `+` or `/`, using `>=` rather
  than `>` for capacity pressure, or reversing the dead/live comparison. Review accepts
  the `+` variant as a harmless heuristic alternative, not a required API behavior.
  The other three introduce unnecessary copies in the spare-capacity profile scenario;
  the separate resource gate guards that cost without coupling unit tests to private fields.
- Additional adapter/default-reader family: 41 mutants, **32 caught, 5 unviable,
  3 timeouts, 1 functional survivor**. Replacing the byte-alignment `%` with `+` disables
  the byte fast path but preserves decoded values; allocation/scaling measurements,
  not value equality, guard this performance distinction.
- Timeouts are recorded separately: an inverted element-progress check with a hostile
  zero-width count, and two retry-condition mutations that loop on permanent scripted
  I/O failures. They are bounded harness detections, not ordinary assertion failures.

The stateful `stream_decode` fuzz target completed 2,000,000 cases on the final runtime
paths (seed 649629597, 419 s, coverage 231, feature count 1617); the slice `decode` target
also completed 2,000,000 cases (seed 1839166121, 27 s). Subsequent edits add regression
tests, profiling, and evidence only. CI's 60-second/two-million-case smoke limit is a
different budget and does not promise two million cases on a hosted runner.

### 11.7 Delivery receipt (2026-09-09 UTC)

The feature merged through [PR #76](https://github.com/RawSocketLabs/rsl/pull/76) as
`d27ad646`; the generated [release PR #77](https://github.com/RawSocketLabs/rsl/pull/77)
merged as `91f87b7b`. Both were merged under `michael-smythe`. Release-plz generated
the versions, dependency requirement, and changelogs; the detached fuzz lock and README
were aligned separately. The first generated-PR CI run caught the stale detached lock;
the corrected exact head passed full local container CI and
[hosted PR CI](https://github.com/RawSocketLabs/rsl/actions/runs/34297854868).
[Main CI](https://github.com/RawSocketLabs/rsl/actions/runs/34298121386) passed before
[release automation](https://github.com/RawSocketLabs/rsl/actions/runs/34298309943) published
both crates. Both 0.5.0 candidate archives were packaged and built together before merge.

Registry archive SHA-256 checksums match the public index:

- `bitsandbytes 0.5.0`: `46785464d3a301ebd5eccdaf32eead1e9f0b7335860c7dcf4d26f08aff5ad22f`.
- `bitsandbytes-macros 0.5.0`: `eede5d552e588d36155a6af4ba4db942daa7beff3f64f6f7004435c5f2f6aca8`.

Both crate-prefixed tags and the archives' VCS metadata identify `91f87b7b`; neither
registry version is yanked. docs.rs serves both 0.5.0 versions. The release allowlist remains
limited to this pair; no other crate was released and no GitHub Release was created.

A clean external consumer, renamed to `wirebits` with `=0.5.0` and `net`/`tokio` features,
resolved both crates from the registry without path dependencies or patches. Its executable
verified fragmented decode, detailed incomplete results, finite EOF, and buffered raw-tail
handoff; strict Clippy and Rust 1.85 checks passed. Post-release patch compatibility checks
against published 0.5.0 pass **223 rules** in each of all/default/no-default feature modes
(31 inapplicable rules skipped). Formatting, strict bnb/macros Clippy, the unchanged pinned
public-API snapshot, denied-warning docs, and actionlint also pass. The documentation/CI-only
follow-up still requires clean-tree container CI and exact-head hosted checks before merge.

The next smallest implementation slice is **SOCKS adoption only**:
replace duplicated framing with the approved buffer/handoff API and prove handshake-to-raw
payload preservation in sync and async paths. Do not combine that adoption with another
parser architecture, client/server expansion, or the independent audit follow-ups.

## 12. Hint-driven whole-message reads (0.6.0)

### 12.1 Scope, contracts, and migration

Baseline: `8c2757db04e3e8997f50698a73723e3fa96172a0` (runtime/macros 0.5.0). Work is isolated
on `feat/bnb-hinted-message-reads`; both existing SOCKS worktrees/indexes are untouched.
Expected release: **bitsandbytes 0.6.0 with bitsandbytes-macros 0.5.0**. Release-plz owns
versions/pins/changelog; no macro expansion change warrants a macros bump.

`MessageStream::read_message` now handles hint-based refills internally. Borrowed
`net::read_message` and `net::read_message_async` take the transport, `BitBuf` and reusable
scratch. They return a complete `T` or `MessageReadError::{Codec(BitError), Io(io::Error)}`,
preserving the original owned I/O source and codec location/field. This changes the existing
method's error type; match the outer variant before inspecting `kind` / `kind()`. Writes and
datagrams retain `BitError`. No second `_hinted` API, status enum, callback framework,
transport owner, runtime or parser-continuation engine is added.

`Incomplete::needed = Some(n > 0)` is a **lower bound**, not an estimate: adding fewer than
`n` physical tail bytes cannot change this attempt to success **or a terminal error** with
the same retained prefix, numeric start cursor, layout, context and codec-visible state.
It does not promise sufficiency or the final frame length. `None` and defensive `Some(0)`
schedule retry after at least one new byte; raw errors are not rewritten. Custom speculative
codecs must use `None` unless the bound is provable. Overstated downstream hints can stall
reads; the contract cannot be enforced on arbitrary user code and is not a memory-safety rule.

Decode precedes scratch validation and transport access. Each read is bounded by scratch
and free retained capacity (since 0.10.x a message that needs at least a scratch more,
and at least 4 KiB, is read straight into the buffer instead, bounded by its hint and the
capacity: §22, phase 5). Even huge hints fill remaining capacity before `BufferFull`,
without an extra read. EOF immediately makes a finite attempt despite an unmet hint; empty
EOF is `Io(UnexpectedEof)`, truncation/custom finite failure is `Codec`. Both normal and finite
success consume final byte padding; core `BitBuf` preserves packed bits. Every successful
read is appended before another await. Cancellation retains accepted bytes, not callback
side effects or protocol-session resumability. Timeouts remain caller-owned.

**Review-driven correction:** `push` can compact/rebase the numeric cursor. Generated absolute
`seek` can then discover an error with less input than the old hint. Both helpers detect cursor
rebasing across `push`, discard that hint and immediately retry. Merely requiring custom codecs
to be rebase-invariant was insufficient: generated positioning is also affected. Applications
with message-relative pointers should still extract a bounded envelope before interpreting it.

`net` adds optional workspace `thiserror`; new `tokio-io` adds optional workspace Tokio and
implies `net`, without requiring `tokio-util`. Existing `tokio` includes `tokio-io` and retains
`BinCodec`. Default/no_std dependency boundaries do not change. `BinCodec` stays stateless and
datagram-compatible. Direct `Source`/`BufSource`/`StreamBitReader` do not become transactional;
`BitBuf` owns whole-message retry. General variable-element/delimiter replay may still be
quadratic; batching a blocked byte-blob read is not full parser resumption.

### 12.2 Audit ledger

Review covers shared runtime/macro dependencies, not only diff lines. The independent
0.5 audit follow-ups in §11.3 remain separate; none is silently waived here.

| Severity / location | Evidence and consequence | Disposition / regression |
|---|---|---|
| High, candidate `net` / `BitBuf::make_room` | Rebased absolute seek can discover an error before a hint expires. | Invalidate on rebase in both drivers; generated seek/assert regression checks exactly one byte consumed. Removing the check fails both tests. |
| High, pre-existing `Incomplete` contract | Estimates cannot justify suppressing attempts that could complete or fail. | Strict lower-bound docs/display and migration; audit producers and propagation, not just terminology. |
| Medium, pre-existing I/O error conversion | `BitError::from(io::Error)` loses ordinary transport sources. | Typed thiserror reader boundary; source-chain/non-ZST pointer-identity regression. Writes unchanged. |
| Reviewed, physical scalar/bulk/seek shortage | IncrementalReader rounds up bit shortage or reports whole-byte shortage before allocation. | Every shorter length with representative contents, aligned/unaligned MSB+LSB, seek/restore and mapped UTF-8 validation. |
| Reviewed, magic dispatch / macro composition | Bytewise magic probing must discover early mismatch; mappings/contexts propagate the first blocked operation. | Long `ABCDE` versus `AX` decode/peek; scalar magic becomes terminal only when complete; stable context/window and mapped-body tests. No macro change. |
| Reviewed, SourceReader / logical bounds | Partial `read_exact` returns a short read, then a typed next-byte shortage; declared region EOF is terminal. | Typed hint/field/position preservation and retained retry; physical versus logical `LimitedSource` tests. No direct-source rollback claim. |
| Low, pre-existing net-only rustdocs | Mock-type links failed without `mock`. | Feature-conditional references use code text; denied-warning net-only docs join all/no-default gates. |
| Medium, test evidence, fixed during review | Bulk helper ignored LSB; ZST identity was weak; fixed fuzz scratch never limited reads. | Explicit `try_pull_with` layout, non-ZST marker and input-selected scratch. Representative patterns are not claimed to exhaust all byte contents. |

### 12.3 Verification and performance gate

Evidence directory: `/tmp/bnb-hinted-checks.tTiIUI` (2026-09-09). Candidate, immutable baseline,
API, MSRV, fuzz and mutation builds use separate targets. Tests follow the repository tiers:
inline component contracts, integration/properties, existing e2e/UI suites and stateful fuzz.
The readers are proven with fixed records, length-prefixed bodies, NUL delimiters, alternative
magic and sub-byte padding independently of SOCKS. Existing DMR, DNS-envelope/DER and context
examples remain compatibility evidence. The fuzz oracle independently models length-prefixed
records and checks all unconsumed bytes after error/handoff; async futures are dropped on
Pending to exercise retention of completed reads.

Predeclared final performance gate: identical nine-case `stream_envelope` harness on both
revisions; 64/1024/16384-byte bodies, chunks 1/64/whole, 20 samples, 300 ms warmup, 2 s
measurement. Rust 1.98.0 / LLVM 22.1.8, Threadripper 7970X, Linux x86-64 system allocator,
CPU 0 pinned, existing powersave/boost settings unchanged. Runs are serialized without
concurrent task builds/fuzzing. Initial concurrent-build runs are exploratory only. Require
baseline repeats within 5%; investigate unexplained candidate regression above 15% (3× noise).
Deterministic buffer/decode/read-count tests run in CI; elapsed-time thresholds do not.

Separate allocation profiling (`bitbuf_bounded --all-features --profile` under
`codecs/bitsandbytes/scripts/profile-allocations.gdb`) passes all 12 phases. Existing bounds,
grow/clone and copy characterizations remain intact. New phases prove zero allocations for
1000 bounded scalar reads, and exactly one 4096-byte decoded-payload allocation for sync
fragmented and async borrowed reads, with zero reallocations. Debug aggregate/error memmove
totals are not payload-copy counts or release-throughput measurements.

Final measurements, post-fix verification and independent approval must be recorded below
before commit. Delivery then requires clean-tree container CI, exact-head hosted PR/main CI,
generated release-PR archives and registry checks. The next smallest slice remains SOCKS
adoption of the published borrowed helper only.

Final serialized timing results (`bnb-05-final-a/b` / `bnb-06-final-a/b`):

| Body / transport chunk | Baseline A / B | Candidate A / B |
|---|---|---|
| 64 B / 1 B | 3.497 / 3.492 µs | 467.88 / 455.65 ns |
| 64 B / 64 B | 118.58 / 119.85 ns | 105.48 / 106.28 ns |
| 64 B / whole | 67.773 / 68.400 ns | 67.335 / 68.468 ns |
| 1 KiB / 1 B | 53.036 / 52.746 µs | 5.536 / 5.558 µs |
| 1 KiB / 64 B | 947.73 / 948.86 ns | 249.47 / 232.70 ns |
| 1 KiB / whole | 93.716 / 94.684 ns | 101.51 / 92.599 ns |
| 16 KiB / 1 B | 838.12 / 843.66 µs | 86.746 / 86.753 µs |
| 16 KiB / 64 B | 14.548 / 14.129 µs | 2.2573 / 2.2450 µs |
| 16 KiB / whole | 731.29 / 719.09 ns | 565.61 / 570.45 ns |

Baseline repeat variation is at most 2.93%. Comparing the slower candidate with the faster
baseline, the largest regression is 8.32%, within 15%; no performance blocker remains.
Candidate 16 KiB/one-byte scaling versus 1 KiB is 15.67× / 15.61× for 16× input. Positive-hint
component tests require two decode attempts despite three one-byte refills; unknown/zero hints
make four, and read overshoot must not trigger an extra transport read. These are CPU/memory
framing measurements, not network throughput. No encoder or general resumable-parser speedup
is claimed. The identical Criterion executable grows 4,756,560 → 4,761,520 bytes (+0.11%);
macros/generated protocol source are unchanged. No cold-build timing claim is made.

Focused manual mutations were applied to an isolated copy, restored after each run: retry
every read, wrapping shortfall subtraction, subtract one instead of the accepted byte count,
ignore rebase, drop appended bytes, and omit final-byte padding. All six compiled and failed
behavioral assertions (2, 4, 3, 2, 18 and 3 failures respectively); none survived. The initial
automatic run found 11 caught, 4 unviable and 6 timeout mutations, with no survivors. Timeout
mutants are recorded as timeouts, not assertion catches. The final post-fix automatic run
tested 23 mutants: **13 caught, 4 unviable, 6 timeouts, zero survivors**. The extra rebase
mutations are caught by the new regressions. Unviable generic replacements require a missing
`T: Default` bound; they are not counted as behavioral proof.

Completed local gates include workspace tests, strict bnb/macros all-target/all-feature Clippy,
the bytes/mock/tokio-io/tokio suites, denied-warning all/no-default/net-only docs, Rust 1.85
workspace/all-feature/renamed-consumer checks and stable bare-metal compilation. Configured
workspace Clippy's 106 warning lines match the immutable baseline exactly; historical bnb
macro warnings are resolved, not waived. Cargo-deny passes advisories/bans/licenses/sources;
yanked wnaf is absent. Actionlint passes. Exact API deltas match in all/default/none modes;
default/none each pass 223 patch-compatibility rules (31 skipped). All-feature major mode
executes zero rules and is not treated as proof; the reviewed exact delta is the release gate.
The development archive builds against registry macros 0.5.0; repeat packaging on the actual
release-plz-generated version. Final fuzz/reviewer/container/hosted delivery evidence remains
required before claiming release readiness.

Final post-fix ASan runs (seed `9062026`) both exited successfully: `stream_decode` completed
2,000,000 cases in 409 s (coverage 323, feature count 2000), and `decode` completed 2,000,000
in 35 s (coverage 245, feature count 643). The final stream run includes variable scratch
limits and both readers after the rebase correction. Generated discoveries were archived
outside the worktree; only existing curated seeds remain. The final all-feature suite passes
185 inline tests plus integration/property/UI/e2e/doctests; focused no-default incremental/DMR
tests and strict net-only/tokio-io Clippy also pass. Public API output matches the committed
snapshot after the final source fixes. Final independent approval and delivery gates follow.

Independent final reviewer approval: **2026-09-09, no remaining findings**. Approval covers
the final runtime/API/feature diff, rebase fix, tests, both two-million-case fuzz runs,
mutations, allocation/performance evidence and reconciled ledger. Only clean-tree container,
hosted and generated-release gates remain. Subsequent source edits require affected checks
and renewed review; green CI alone does not waive the pre-commit gate.

### 12.4 Delivery receipt (2026-09-09 UTC)

[PR #80](https://github.com/RawSocketLabs/rsl/pull/80) delivered the reviewed source as
`cd5db374`, after full clean-tree container and hosted CI. PR #79 was superseded only to
wrap its commit-message body/footer for commitlint; its published branch was preserved,
and the replacement Git tree was identical. No source or published history was rewritten.

Release-plz generated runtime **0.6.0** in [PR #81](https://github.com/RawSocketLabs/rsl/pull/81);
macros remain **0.5.0**. The generated PR's initial CI exposed its stale detached fuzz lock.
Refreshing only the runtime version fixed the locked-metadata gate. README/release guidance
was aligned, both commits passed local commitlint, and the independent reviewer approved
the eight-file candidate. Strict bnb/macros Clippy, workspace/all-feature tests, clean archive
verification, full clean-tree container CI and fresh hosted PR CI passed before merge.
The 254-file runtime archive builds against registry macros 0.5.0, not a local macro patch.

PR #81 merged as `6bcbb949dba082fcff5c0f0335b3be2bff909dd3` by `michael-smythe`.
[Main CI](https://github.com/RawSocketLabs/rsl/actions/runs/34315930303) passed for that exact SHA;
[release automation](https://github.com/RawSocketLabs/rsl/actions/runs/34316114611) then published
[bitsandbytes 0.6.0](https://crates.io/crates/bitsandbytes/0.6.0). Crates.io records
`michael-smythe` as publisher at `2026-09-09T05:45:38Z`; the version is not yanked.
The annotated `bitsandbytes-v0.6.0` tag and the registry archive's VCS metadata both resolve
to the release merge SHA. Downloaded archive, sparse index and registry API checksums agree:
`8b8c57b18675d29aac95d3decfc16d201592817deeff6d105d46f9a9813d4f40`.
Macros 0.5.0 remain the latest macro release; no macro 0.6 tag or GitHub Release was created.

The [versioned async reader docs](https://docs.rs/bitsandbytes/0.6.0/bnb/net/fn.read_message_async.html)
are available. A fresh external registry-only consumer renames the dependency to `wire`, uses
`default-features = false` with `tokio-io`, and verifies generated length-prefixed records,
borrowed sync/async readers, buffered success with empty scratch, clean EOF classification,
and lossless raw handoff. It passes stable and Rust 1.85 execution plus strict all-target Clippy;
its lock/tree resolve registry runtime 0.6.0 and macros 0.5.0 with no path patch.

Post-release CI restores `--release-type patch` against published 0.6.0 in all/default/none
feature modes; each local comparison passes 223 rules (31 skipped), and the public-API
snapshot remains unchanged. The temporary delta checker is retired; all reviewed delta snapshots remain
historical evidence and the standing public-API snapshot remains enforced. The next smallest
slice is SOCKS adoption of the published borrowed readers, preserving its existing session,
error and handoff policies. Neither dirty SOCKS checkout nor the unrelated PNGs was changed.

## 13. Type-attributed enum dispatch diagnostics (0.7.0)

### Contract and consumer boundary

Closed `#[bin]` enums now report `ErrorKind::NoMatchingVariant(Box<EnumDispatchError>)`
instead of an unstructured `Convert` string when dispatch exhausts its alternatives.
`BitError::dispatch_error_for::<T>()` checks the actual enum's `TypeId`; diagnostic names
are not identity. The details expose `enum_name()`, `dispatch_kind()`, and `observed()`.
`DispatchKind` distinguishes magic, external tag, and tag-then-magic dispatch;
`DispatchValue` owns either an interpreted `u128` or a fixed-width byte-string `Vec<u8>`.
These extensible enums and the details remain `no_std + alloc`, without new dependencies.
The existing construction-only `BitEnum::TryFrom` error, `UnknownDiscriminant`, is unchanged.

Only terminal closed tails in ordinary decode and `peek_variant` construct these details.
Uniform magic dispatch captures the already-read value; arbitrary context tags and
variable-width probes capture nothing. `None` means **not captured**, never incomplete.
A hybrid miss reports its final wire magic, not the tag. No new selector conversions,
trait bounds, input reads, rewinds, or payload copies are introduced. Prefix/signature and
`decode_as_*` verification errors retain their existing `Convert` kind; nested payload
errors propagate unchanged. Catch-all/fallback success, declaration order, shortfalls,
finite EOF, offsets, and innermost field attribution keep their existing contracts.

Current generated enums reject all generic parameters, so actual `TypeId::of::<Self>()`
needs no marker/identity abstraction. Generic or lifetime-bearing enum support must revisit
that assumption. The doc-hidden constructor is macro plumbing, not a provenance/security
boundary for manually constructed errors. Runtime and macro releases must be coordinated:
the new expansion requires this runtime helper; no versions are changed in this candidate.

The motivating consumer is SOCKS: on its unmerged framing branch, `From<bnb::BitError>`
turns only an `Endpoint`-attributed integer fitting in `u8` into `UnsupportedAddressType`,
replacing header probing in blocking, Tokio, and Mio. That adoption is **not part of this
change**; trunk SOCKS only updates the one test that matched the old `Convert` text.

### Performance gate and findings

The immutable source baseline is Git tree `ddc8aceaf749287dd3ae6265dc867eced5bcf77a`
(the existing staged SOCKS candidate over commit `9b29a2a88816d6b0184590a306f7d09acc39ddd7`).
The original checkout/index is preserved. The new `enum_dispatch` benchmark fixtures run
unchanged against baseline and candidate in optimized bench profiles: valid/invalid numeric
and four-byte magic, one invalid input per 16 attempts, and 16 coalesced/fragmented messages.
Instrumentation runs separately through the benchmark's `BNB_DISPATCH_PROBE=1` entry point.
System-allocator calls are counted with GDB; Criterion timings have no allocation hooks.

Environment: Rust 1.98.0 / LLVM 22.1.8, Linux x86_64, Threadripper 7970X, system allocator,
default bnb features, CPU affinity 2. Screening runs use 40 samples, 0.2 s warmup, and
0.6 s measurement; final alternating comparisons use 60 samples, 0.5 s warmup, and 1 s
measurement, repeated three times per executable. Both use 10,000 resamples and no plots.
Repeatable regressions beyond baseline variation require optimization or explicit approval,
not a silent larger budget. The user retained this strict gate. (Since 2026-11-12 the
gate is applied by tier: §22, phase 0.)

The initial inline constructor preserved `BitError` (80 bytes), `ErrorKind` (48 bytes),
and representative `Result` sizes (80 bytes), with zero successful-path allocations.
Numeric failure used one allocation of 80 bytes versus the baseline string's 41 bytes;
four-byte failure used two allocations totaling 84 bytes versus one of 39 bytes. Across
three alternating runs, median rejection time rose from 20.797 to 25.019 ns (numeric) and
25.495 to 33.482 ns (bytes). This is a **new acceptance blocker**, not an accepted tradeoff.
The first cold constructor was slower still; inlining reduced, but did not remove, the cost.
Sharing immutable name/kind metadata in a promoted static tuple reduces the allocated
details; type identity and owned observed values remain per error.

That compaction reduces the box to 64 bytes (numeric total 64; four-byte total 68),
with no successful-path allocation or inline error/result growth. It alone does not
clear the rejection-latency gate. An inlining trial on `read_byte_array` produced no
benefit and was reverted. A generated context-free `BitDecode::bit_decode` inline hint
improved rejection time but caused a repeatable 7% whole-frame buffered regression; it
was also reverted. The final experiment instead emits the same decode body through
`decode_exact_with` at the context-free enum's slice entry point, leaving the trait method
unchanged. That preserves slice improvements but does not resolve the buffered regression.

Final measurements below are medians of the three runs' arithmetic mean estimates, in
nanoseconds. Mixed and buffered cases process 16 messages per iteration, not one:

| Case | Baseline | Candidate | Change |
| --- | ---: | ---: | ---: |
| Integer success | 9.873 | 9.804 | -0.7% |
| Integer rejection | 20.237 | 14.551 | -28.1% |
| Four-byte success | 17.866 | 13.705 | -23.3% |
| Four-byte rejection | 25.312 | 23.911 | -5.5% |
| Mixed, one rejection per 16 | 160.910 | 151.121 | -6.1% |
| Buffered, one-byte chunks | 1238.426 | 1234.439 | -0.3% |
| Buffered, five-byte whole frames | 240.032 | 258.578 | **+7.7%** |
| Buffered, 80-byte coalesced input | 187.336 | 188.955 | +0.9% |

The whole-frame baseline means span 239.832–246.280 ns; candidate means span
254.382–264.848 ns. These ranges do not overlap. Improvements elsewhere do not cancel
this failed acceptance gate. No SOCKS session throughput improvement is claimed.

Separate GDB accounting confirms no success allocations, no reallocations, one 64-byte
allocation per integer rejection, and two allocations totaling 68 bytes per four-byte
rejection. `BitError`, `ErrorKind`, and representative results remain 80/48/80 bytes.
The benchmark executable's text grows from 3,180,442 to 3,181,846 bytes (`size`); this is
a Criterion-heavy executable comparison, not a consumer-library code-size guarantee.

Reproduction: build `cargo bench --offline --locked -p bitsandbytes --bench bitstream_bench
--no-run` against each source tree with the same benchmark harness. Run each saved
executable with `taskset -c 2 <executable> --bench enum_dispatch --warm-up-time 0.5
--measurement-time 1 --sample-size 60 --nresamples 10000 --noplot --save-baseline <name>`.
Run `gdb -q -batch -x codecs/bitsandbytes/scripts/profile-enum-dispatch.gdb --args <executable>`
separately; set `BNB_PROFILE_BASELINE=1` for the old implementation. Local evidence is in
`/tmp/rsl-dispatch.W49EOH/slice-gate-{baseline,slice}-{1,2,3}.log` and
`slice-gate-allocations.log`; Criterion estimates are under the corresponding
`dispatch-slice-gate-*` directories in `target/criterion/enum_dispatch`.

The new empty-buffer regression initially assumed a known shortfall and truncation at EOF.
Existing `BitBuf` intentionally skips the codec for empty input: `try_pull` reports an unknown
hint and `pull_eof` reports clean EOF. The test oracle is corrected without changing runtime
behavior. The final candidate passed the 43 tests in `bin_dispatch_error`, `bin_enum`, and
`incremental` with all features, and strict all-target/all-feature Clippy for bnb, its
macros, and SOCKS. Formatting was applied. These commands ran offline on 2026-09-19.
The strict Clippy run reported no issues; the historical macro warnings are not a failure
of this candidate. `cargo deny` was not rerun, so the historical yanked-wnaf advisory is
not newly verified or resolved here.

**Superseded status (2026-09-19): acceptance was blocked on performance.** That candidate
stayed isolated and unstaged; independent review found no correctness, safety,
macro-hygiene, identity, conversion, or test-oracle findings but withheld approval on the
whole-frame regression. The 2026-09-20 revision below replaces this status.

### Review revision (2026-09-20)

A four-lens review with independent refutation changed the candidate and its gate.

**Findings ledger.** All are new to this candidate unless marked.

| Location | Finding and evidence | Disposition |
| --- | --- | --- |
| macro `decode_exact` via `decode_exact_with` | Unrelated slice-entry specialization; sole source of every improvement in the table above, masking the constructor's cost. Mutating it to `decode_peek_with` kept the full bnb and SOCKS suites green: no enum `TrailingBytes` test. | Removed from this candidate. Reintroduce only as its own change with that test and its own gate. |
| `BitError::no_matching_variant::<T>` `#[inline]` | Put `__rust_alloc` in every closed enum's decode frame: every preserved pre-fix executable has `push/pop %r15` and a reload in `decode_with::<IncrementalReader>` (0x1ef bytes or more, against the baseline's 0x1df). | Now non-generic `#[cold] #[inline(never)]`; the macro passes `TypeId::of::<Self>()`. The candidate's `decode_with` is 0x1df bytes with no `%r15`; `BitBuf::pull` is instruction-identical to baseline. |
| `buffered_chunk_*` bench cases | Whether a build inlined `BitBuf::pull` into the Criterion closure moved the same library code between 207–226 and 237–268 ns. | Measured unit moved to `#[inline(never)] drain`; `buffered_invalid` added for rejection through `BitBuf`, the path stream consumers use. |
| This section, "ranges do not overlap" | True only within one session: the same baseline binary spans 236.9–255.0 ns (7.6%) across saved sessions, and no numeric budget was recorded (RELEASING gate step 2). | Budget recorded below; those sessions predate the `drain` harness. |
| `Display` of `DispatchValue::Bytes` | `{:02x?}` rendered `[58, 59]`: unprefixed hex. Only the integer form was tested. | Renders `0x5859`; `unit::display_no_matching_variant` covers integer, bytes, and uncaptured forms. |
| `bin_dispatch_error` | No positive miss for a prefixed closed enum; one assertion could not fail. | `prefixed_enum_miss_is_attributed_after_its_prefix` added; the `clone` equality removed. |
| `nostd-check::dispatch_diagnostics` | Asserted in a build-only crate; the assertions never ran. | Returns the captured values; documented as a compile proof. |
| `public-api.txt` | Lacked the new items; the CI comparison is byte-exact. | Regenerated with `cargo +nightly-2026-06-17 public-api -p bitsandbytes --all-features` (cargo-public-api 0.52.0): 264 added lines, none removed. |
| Tag plus variable-width magic under `ctx` (pre-existing) | Does not compile (`S: SeekSource` unsatisfied in the `DecodeWith` impl), so `(TagOrMagic, None)` is unreachable. | Recorded in `ROADMAP.md`; not changed here. |

**Release requirement.** The expansion calls a runtime helper that 0.6.0 lacks, while 0.6.0
accepts macros `^0.5.0`. Macros must ship as **0.6.0**, never 0.5.x, and terminal misses
changing from `Convert` to `NoMatchingVariant` is an observable behavior change: both crates
take a breaking (`!`) marker, following the 0.4.0 and 0.5.0 precedent. No versions are changed
in this candidate.

**Gate and budget.** The success-path gate is deterministic: user-space instruction counts
equal to within 0.001 per message on a fixed-work probe (`BNB_DISPATCH_PROBE=buffered`, 1,000,000 whole-frame drains of
16 messages, `perf stat -e instructions:u,cycles:u`, CPU 2). Criterion wall-clock is secondary
with an 8% budget. That figure is inherited: it is the 236.9–255.0 ns spread of baseline
whole-frame means across the saved pre-`drain` sessions, and sits inside the 15%
whole-message budget §12.3 predeclared. Under this revision's harness the baseline repeats
within 3.3% (245.58–253.66 ns). Rejection is terminal for a stream consumer, so its latency is
recorded, not gated; the owner directed this tradeoff on 2026-09-20 in exchange for a success
path with no added instructions or spills.

| Fixed-work probe, three alternating runs | Baseline | Candidate |
| --- | ---: | ---: |
| Instructions | 6,051,536,706–7,300 | 6,051,537,669–7,977 |
| Cycles | 1,264.2–1,284.4 M | 1,247.8–1,252.0 M |

That is 378 instructions per message in both (the difference is under 0.0001 per message),
and 1–3% fewer cycles in the candidate. The typed error adds no success-path instructions;
the candidate's `decode_with` reserves 32 more stack bytes (`sub $0x58` → `sub $0x78`).

Criterion medians of three alternating runs, in nanoseconds: the CLI point estimates (slope
where Criterion fits one), same settings and baseline tree as above, both trees built with
this revision's final harness:

| Case | Baseline | Candidate | Change |
| --- | ---: | ---: | ---: |
| Integer success | 9.851 | 9.834 | -0.2% |
| Integer rejection | 19.768 | 30.859 | +56.1% |
| Four-byte success | 17.772 | 17.406 | -2.1% |
| Four-byte rejection | 27.081 | 39.592 | +46.2% |
| Mixed, one rejection per 16 | 163.71 | 159.77 | -2.4% |
| Buffered, one-byte chunks | 1226.6 | 1243.2 | +1.4% |
| Buffered, five-byte whole frames | 246.61 | 243.55 | -1.2% |
| Buffered, 80-byte coalesced input | 187.55 | 189.66 | +1.1% |
| Buffered rejection | 15.717 | 22.986 | +46.2% |

No success-case median exceeds the baseline's slowest repeat. Running the
whole-frame case alone gives baseline 247.07–251.14 ns and candidate 239.06–245.70 ns. An
earlier build of this same revision, differing only in a probe function that Criterion never
calls, showed +5.6% on that case in the full group and the opposite order alone: the case
follows executable layout, not library work, which is why instruction counts are the gate.
Rejection costs 7–13 ns more per terminal miss. GDB accounting on the final candidate
matches the counts recorded above: 0 / 1000 / 0 / 2000 allocations, 64,000 and 68,000 bytes, no reallocations;
`BitError`/`ErrorKind`/`Result` remain 80/48/80 bytes.

Evidence, all produced by the two executables whose digests are in `executables.sha256`
under `/tmp/rsl-gate.piGQor/`: `gate-{baseline,cold}-{1,2,3}.log`,
`alone-{baseline,cold}-{1,2,3}.log`, `perf-{baseline,cold}-{1,2,3}.log`,
`allocations-cold.log`, and `disasm-summary.txt`. `superseded/` holds the earlier build's
logs. The directory is local and does not survive a reboot.

**Qualification (2026-09-20).** `scripts/ci-act.sh pre-push --base 9b29a2a8` (since replaced by
`scripts/ci-local.sh`) ran against a
throwaway commit of this candidate in a scratch clone; the checkout and index were untouched.
Every selected check profile's step passed: blessed, bnb, facades, msrv, network, no-std,
package, public-api, semver, socks, workspace, rust-dsdcc, rust-skills, and usdr. Two
failures are not the candidate's:

- `deny` fails on RUSTSEC-2026-0285 (rustls 0.23.41), already in trunk's `Cargo.lock` at
  `9b29a2a8`. Baseline-only, and it blocks any push until trunk resolves it.
- The `stream_decode` fuzz job died in act's `Swatinem/rust-cache` step
  (`ERR_MODULE_NOT_FOUND` for `dist/cleanup-BPghO_DY.js`, while several jobs were cloning
  that action) before fuzzing. Its sibling fuzz jobs ended failed without reaching a fuzz
  step, and act labelled the passing check jobs failed as well.

The bnb, SOCKS, and first two crypto targets were therefore run on the host with CI's
arguments (`-max_total_time=60 -runs=2000000`); none crashed (exit 0 observed for each,
recorded only in `fuzz-stream_decode.log`): bnb `decode` 2,000,000 runs and
`stream_decode` 591,273; SOCKS `session` 293,738 and `mio_session` 159,467; crypto
`digest_fragmentation` 277,366 and `aead_open` 19,053. The remaining crypto and pki targets
were not run; this candidate does not touch them.

Focused mutation (`cargo mutants` 27.1.0): runtime `no_matching_variant`,
`dispatch_error_for`, `EnumDispatchError`, and `Display for BitError` gave 10 caught, 4
unviable, 1 missed; macro `bin_enum` gave 4 caught, 51 unviable, 7 missed. No survivor is on
a line this candidate changes: the runtime one is the `Incomplete` hint guard (`*n != 0`), and
the macro ones are feature-selection conditions at `bitstream.rs` 3949, 3978, 3981, 4100,
and 4119. They are pre-existing and belong in `ROADMAP.md` follow-up, not this change.

**Status.** No performance blocker remains under the recorded gate, and the correctness
tiers above are complete. Independent review reproduced every figure in this revision from
the evidence. This change lands on trunk alone as `feat/bnb-dispatch-diagnostics`. The
measurements and qualification above were taken with the unmerged SOCKS framing work
underneath; that work touches nothing under `codecs/bitsandbytes/`, so the bnb sources measured are
the ones committed here, and the trunk-only tree was re-qualified separately (below). The
trunk `deny` advisory is fixed separately by the rustls 0.23.45 lockfile update. No release
is made here: release-plz derives both breaking versions.

**Trunk-only qualification (2026-09-21),** on `origin/main` `9b29a2a8` plus this change:
`cargo fmt --all --check`; strict all-target, all-feature Clippy for bnb, macros, and SOCKS;
`cargo test --workspace` (1491 passed) and bnb plus SOCKS with all features (696 passed);
denied-warning docs; `cargo +1.85.0 check`; the `thumbv7em-none-eabi` `nostd-check` build; a
byte-identical `public-api.txt`; and trunk SOCKS `session` fuzz, 2,000,000 runs, exit 0
(`fuzz-trunk-socks-session.log`). `scripts/ci-act.sh pre-push` was not repeated on this tree.

## 14. Owned buffered seek reader (0.7.1)

### Scope and contract

Motivating consumer: a camera RAW/TIFF parser that reads `u16`/`u32` values at arbitrary
offsets from a `BufReader<File>`, with byte order chosen at run time from the `II`/`MM`
header. `SeekReader::read_bits` seeks to an absolute offset and allocates a `Vec` before
every read, and `BufReader::seek` always discards its buffer, so each value cost a seek plus
a buffer refill. Two changes, one concern:

- `SeekReader` reads into a 17-byte stack buffer (7 leading bits plus 128) instead of a `Vec`.
  Validation, errors, offsets, and the per-read absolute seek are unchanged; the shared read
  path (`ByteSpan`) is private.
- New `BufSeekReader<R: Read + Seek>` owns a `BufReader<R>` and tracks the stream offset. It
  seeks only when a read does not start where the last ended, and then with
  `BufReader::seek_relative`, which keeps the buffer when the target is buffered. The offset
  is unknown (absolute seek next) before the first read and after any failed seek or read.
  It implements `Source`/`SeekSource`, takes any `Layout` at construction, and offers
  `new`/`with_layout`/`get_ref`/`into_inner`. It has no `get_mut` and no `Clone`: both would
  let the stream move behind the tracked offset.

`SeekReader` keeps the per-read absolute seek on purpose. It derives `Clone` and accepts any
`R`, including shared-cursor readers such as `&File`, where a remembered position is wrong
as soon as another handle reads. Skipping its seek conditionally would be a silent
correctness regression for those users; the owned type makes the assumption explicit.

### Findings ledger

| Location | Finding and evidence | Disposition |
| --- | --- | --- |
| `SeekReader::read_bits` (pre-existing) | One `seek` + one `read` syscall and one allocation per value; through `BufReader` each seek also discards the buffer. 1.25 µs per sequential read (bench below). | Allocation removed; the seek stays (shared-cursor contract). `BufSeekReader` added for owned readers. |
| `SeekReader` over `&File` (pre-existing contract) | No test guarded the shared-cursor case that forbids the skip-seek optimisation. | `seek_reader::interleaved_readers_over_one_shared_file_each_read_their_own_bytes`: two readers (one a clone) over one `&File`. |
| `ErrorKind::Io` (pre-existing) | Carries only `io::ErrorKind`; the source error is lost. Carrying it would be breaking and grow `BitError`. | Not changed. Consumers map `UnexpectedEof` themselves. |
| Seek-reader EOF detail (pre-existing) | Both seek readers report `UnexpectedEof { remaining: 0 }` whatever remains; a zero-width read at a sub-byte offset reads the next byte and can fail, unlike `BitReader`. | Kept for exact parity; recorded in `ROADMAP.md` (I/O ladder). |
| `BufSeekReader::get_ref` (new) | A `&File` borrowed out can move the OS offset and stale the tracked position, as with `BufReader::get_ref`. | Documented on the method and the type; no `get_mut`. |
| `BufSeekReader` given a `BufReader` (new) | Double-buffers: correct but copies twice. | Documented: pass the unbuffered reader. |
| UI snapshots | `ui_seek` and `ui_countprefix_seal` list `SeekSource`/`Sealed` implementors, so a new implementor changes the expected text. | Re-blessed with `TRYBUILD=overwrite` under default features (the harness's gate); diff is the added `BufSeekReader<R>` line and one line pushed into `and $N others`. |

### Tests

- `tests/seek_reader_parity.rs` (`property`, 512 cases): random 0–20,000-byte files (several
  8 KiB buffers), all four layouts, 1–63 random operations (seeks, `read_bits(0..=130)`, `u16` and `u32`
  reads) against `SeekReader` as the reference: values, errors, and `bit_pos` equal at every
  step. Every successful value is also checked against `BitReader` over the same slice, an
  oracle independent of the shared `ByteSpan` path.
- `bitstream.rs` `component::buf_seek_reader`: sequential 64 KiB of `u16`s does exactly one seek
  and at most one underlying read per 8 KiB; a backward read inside the buffer does no I/O; a
  jump out of the buffer seeks once per jump and stays correct; read and seek failures and EOF,
  `TooWide`, `PositionOverflow`, and zero-width cases equal `SeekReader`'s, and a failed read
  re-seeks on retry.
- Mutation: `cargo mutants` over `SeekReader`/`ByteSpan`/`BufSeekReader`: 22 caught, 5
  unviable, 0 missed. Two hand mutations cargo-mutants cannot express were also caught:
  keeping the stale offset after an error (the property test, minimal input 3 bytes) and a
  buffer-discarding relative seek (the backward-read test).

### Performance

Environment: AMD Ryzen Threadripper 7970X, Linux 7.1.9 x86_64, rustc 1.98.0 / LLVM 22.1.8,
system allocator, Criterion 0.8 defaults (100 samples, 3 s warm-up, 5 s measurement),
`--features net`, 2026-09-25. Baseline is `main` `e4d1eda7`. `benches/seek_bench.rs` reads a
1 MiB file in the page cache, with little-endian layout chosen at run time. `SeekReader` wraps
`BufReader<File>` (8 KiB); `BufSeekReader` owns the `File` and its own 8 KiB buffer.
Sequential: 10,922 `u16`+`u32` pairs from offset 0. Scattered: 4,096 random offsets, one pair each.

| Case | `SeekReader<BufReader<File>>` before | same, after | `BufSeekReader<File>` |
| --- | ---: | ---: | ---: |
| sequential (21,844 reads) | 27.249 ms | 27.892 ms | 0.212 ms (128x) |
| scattered (8,192 reads) | 10.510 ms | 10.693 ms | 5.310 ms (2.0x) |

The 128x is against the `SeekReader<BufReader<File>>` shape the motivating consumer uses, in
which every seek discards an 8 KiB buffer and refills it. A plain `SeekReader<File>` avoids the
refill (`seek_reader_unbuffered`; one CPU-2-pinned run, 30 samples, 3 s): sequential
21.365 ms and scattered 7.989 ms, against `BufSeekReader`'s 0.215 ms (99x) and 5.404 ms
(1.5x) in the same run.

The unpinned full-suite rerun reported `SeekReader` +2.4% (sequential) and +1.7%
(scattered). A pinned alternating rerun reversed that: baseline and candidate executables
alternated four times on CPU 2 (`--warm-up-time 1 --measurement-time 3 --sample-size 30`).
Median of means: sequential 27.277 → 26.838 ms (−1.6%), scattered 10.519 → 10.408 ms
(−1.1%). `BufSeekReader` in the same runs: 0.210 ms and 5.368 ms. The stack buffer is a
noise-level gain for `SeekReader`, whose two syscalls and buffer refill per value dominate.
Scattered `BufSeekReader` reads still pay one `lseek` plus a refill per jump outside the
buffer, the floor for an 8 KiB buffer over a 1 MiB file.

Protocol-facing paths do not call the changed code. The unpinned full-suite rerun showed
spurious ±5–25% moves in `incremental_envelope`/`stream_envelope`, so the affected groups
were re-measured as alternating baseline/candidate executables pinned to CPU 2 (`taskset -c 2
<bench> --warm-up-time 0.5 --measurement-time 1 --sample-size 60`, three rounds each). All
medians fall within the baselines' own run-to-run ranges (largest: `buffered_chunk_80` +5.8%,
ranges 186.9–200.3 vs 191.1–201.5 ns). Deterministically, every `bnb::` symbol in the
`message_bench` (49) and `bitstream_bench` (43) executables has the same size in both builds;
`message_bench` text differs by 4 bytes, outside `bnb`.

### Verification (2026-09-25, offline)

`cargo fmt --all --check`; `cargo clippy -p bitsandbytes -p bitsandbytes-macros --all-targets
--all-features -- -D warnings`; `cargo test -p bitsandbytes` with default, `bytes`, `mock`,
`tokio-io`, and all features; denied-warning `cargo doc` (all features, and no default
features); `cargo build -p bitsandbytes --no-default-features`; the `nostd-check` build for
`thumbv7em-none-eabi`; `cargo +1.85.0 check` for the workspace, bnb with all features, and
`nostd-check`; a byte-identical `public-api.txt` (`cargo +nightly-2026-06-17 public-api`
0.52.0: 114 lines added, none removed); `cargo semver-checks` 0.50.0 against 0.6.0,
`--release-type patch`, for all three feature modes ("no semver update required"); `cargo
package --offline` for both crates; `cargo clippy --workspace --all-targets` (no bnb
warnings; existing protocol/rawsock lint debt unchanged); and `cargo test --workspace`.
All passed. Not run: `cargo deny` (needs the advisory database from the network), bnb fuzzing,
and `scripts/ci-act.sh pre-push`, which must run before pushing.

Independent review (read-only reviewer agent, post-fix diff): no blockers. Addressed: the
parity claim now states that `R` must support `SeekFrom::Current`; the property test checks
every bit-consuming success against `BitReader`, since both seek readers share `ByteSpan`
(a hand mutation forcing MSB order in `ByteSpan::read` now fails it); the unbuffered
`SeekReader<File>` row was added; the buffer-count bound derives from the observed
`BufReader` capacity. The bnb checks above were re-run after these fixes. Library changes
after the workspace run were doc comments only, so the workspace suite and semver-checks
were not repeated.

## 15. Stateless slice prefix decoding (0.8.0)

### Scope and contract

A caller that owns its receive buffer could not decode one message from a `&[u8]` and learn
whether a failure meant "short" or "bad" without copying into a `BitBuf`: `peek` decodes
through a plain `BitReader`, so a short slice is a definitive `UnexpectedEof`. The existing
`decode_attempt` (behind `BitBuf::try_pull`/`pull_eof` and `BinCodec`) already makes that
distinction. This change exposes it, with no new decode logic and no feature flag:

- Runtime: `bitstream::decode_prefix_with<T: DecodeWith<A>, A>(bytes, layout, args, eof)` and
  `decode_prefix<T: BitDecode>(bytes, layout, eof)`, both `-> Result<(T, usize /* bits */), _>`,
  both `#[doc(hidden)]` macro plumbing like the neighbouring slice helpers, so outside the
  tracked public API. `decode_prefix` joins `__private`.
- Generated, beside `peek`/`decode_exact` on every context-free decode surface (`#[bin]`
  struct and bare `#[derive(BitDecode)]` via `gen_decode`, enum, mapped, codec newtype):
  `decode_prefix(&[u8])` and `decode_prefix_eof(&[u8])`,
  `-> Result<(Self, usize /* bytes, rounded up */), BitError>`. Two methods rather than an
  `eof: bool` argument, following `try_pull`/`pull_eof`.
- Context types get no generated form: they have no generated `peek` either. They use
  `BitBuf::try_pull_with`/`pull_eof_with`. A documented public `decode_prefix_with` would be a
  separate API decision.

Semantics are `decode_attempt`'s at cursor 0: shortage is `Incomplete` (lower-bound byte hint)
or, with `eof`, `UnexpectedEof`/`IncompleteAtEof`; `LimitedSource` boundaries and codec errors
stay definitive; zero progress, including a zero-width message on empty input, is
`NoProgress`. Empty input is not special-cased (unlike `try_pull`'s `needed: None` and
`pull_eof`'s `Ok(None)`): it is the zero-length truncation. Each call replays from the slice
start; positions are slice-relative.

### Findings ledger

| Location | Finding | Disposition |
| --- | --- | --- |
| `peek` docs (pre-existing) | Nothing said a short slice is indistinguishable from malformed input. | Generated and runtime `peek` docs now say it is not a prefix decoder and point at `decode_prefix`. |
| Byte rounding (new) | Whole-byte `consumed` counts final-byte padding, so back-to-back sub-byte messages cannot be walked with it. | Documented; `decode_iter`/`BitBuf` remain the packed-sequence tools. |
| Replay cost (new) | Repeated attempts on a growing slice redo the decode, as `BitBuf` attempts do. | Documented in `guide::io`; callers should retry only after `needed` more bytes. |

### Tests

`component::incremental::prefix_*` (runtime: consumed bits with a tail, shortage with and
without EOF, empty input, `NoProgress`, a `LimitedSource` overrun inside a longer slice,
custom `Incomplete` becoming `IncompleteAtEof`); `tests/bin_decode_prefix.rs` (`macro_`:
every truncation of a counted record is `Incomplete` with a hint that stays within the true
length, `_eof` and `peek` agree on the definitive error, bad magic, a `parse_with` region
overrun, 12-bit rounding, empty input, and the enum/mapped/codec-newtype surfaces).

Performance: no existing path changed; the new functions are thin wrappers over
`decode_attempt`. Not benchmarked. Not run: mutation testing, fuzzing, MSRV (the current
lockfile needs a newer rustc than 1.85 locally), and `scripts/ci-act.sh pre-push`.

## 16. Bulk byte runs (0.8.0)

### Scope and contract

Evidence: a `perf` profile of a SOCKS client handshake was dominated by encoding
`UsernamePasswordRequest` (two `count_prefix = u8` `Vec<u8>` fields): its `bit_encode` ~11%,
`BitWriter::write_bits` ~28%, `write::<u8>` ~13%. Generated encode wrote every `Vec` element
through `bit_encode`, and the slice reader's `read_bytes` was the per-byte default, so
`decode_vec`'s `u8` override (§11.2) gained nothing on `BitReader`.

- New defaulted `BitEncode::encode_slice(&[Self], sink)`, the dual of `decode_vec`: per element
  by default, `Sink::write_bytes` for `u8`. Generated context-free `Vec` fields (struct and enum
  variant) call it; context-carrying elements keep their loop. The `[u8; N]` helpers use
  `write_bytes`/`read_into`.
- `BitWriter`/`BytesWriter` override `write_bytes`; `BitReader`, `BitBuf`, and `BytesReader`
  override `read_bytes`/`read_into`; the private incremental reader overrides `read_into`.
  The fast path is a plain copy taken only when the cursor is byte-aligned and, for reads,
  the whole run is available: aligned whole bytes are identical in both bit orders, and byte
  order does not apply to one byte. Every other case calls the same private per-byte loop the
  trait default uses, so for `BitReader`/`BitWriter` values, errors, positions, fields,
  partial fills, and cursors are the per-byte loop's by construction. `BitBuf`/`BytesReader`
  run that loop on a `BitReader` at their cursor, which equals their own per-byte path only
  because their `read_bits` is that same reader; the component tests below pin it.

**Why a trait hook, not matching `u8` in the macro.** It reaches aliases (`type Octet = u8`)
and generic element types, which a token match cannot see, and it repeats the shipped
`decode_vec` decision rather than guessing types by spelling. `encode_slice` is public and
documented like `decode_vec` (additive `public-api.txt` lines only); a `doc(hidden)` method
would still be overridable and semver-visible, only undiscoverable. It is an associated
function, so it cannot collide with downstream method-call syntax; the remaining risk is a
`T::encode_slice(..)` path call becoming ambiguous (E0034) for a type that also implements
another in-scope trait with that name — Cargo's "possibly-breaking" defaulted-item class,
acceptable for 0.x. Macro and runtime stay paired: the runtime pins `bitsandbytes-macros`
exactly.

### Findings ledger

| Location | Finding and evidence | Disposition |
| --- | --- | --- |
| Generated `Vec` encode (pre-existing) | Per-element `bit_encode` and one `write_bits` per byte; 1.50 µs for 2×255 bytes (below). | `encode_slice`; 98 ns. |
| `BitReader::read_bytes` (pre-existing) | Per-byte default, so `decode_vec::<u8>` was not bulk on the slice path; 570–592 ns. | Aligned copy; 30 ns. |
| `IncrementalReader::read_bytes` (pre-existing contract) | Reports the whole shortfall before allocating, unlike the per-byte loop. | Unchanged and outside the equivalence claim; its `read_into` is a pure fast path. |
| `LimitedSource`, `StreamBitReader`, `BufSource`, `SeekReader`, `BufSeekReader` (pre-existing) | Still per byte. Delegating `LimitedSource::read_bytes` would change hints over the incremental reader or allocate from a logical window; stream sources have mid-run `BufferFull`/I/O semantics. | Correct as is; tracked in `ROADMAP.md`. |
| Unaligned runs (new boundary) | Still per byte; no shift-merge path. | Tracked in `ROADMAP.md`. |
| `BytesReader` cursor past its end (new edge) | A seek may pass the end; an empty run must still succeed without touching input. | Empty-run guard; covered by the equivalence test. |
| `Source::read_bytes`/`read_into`, `Sink::write_bytes` defaults (new coverage gap) | Five `cargo mutants` survivors: no test sent a byte run through a source or sink that keeps the defaults. | `TinySource`/`TinySink` default-run tests (unaligned, EOF) and a `StreamBitReader` decode of `Vec<u8>` and `[u8; N]` against the slice path. |
| `aligned_run`, `BitWriter::write_bytes` alignment checks (equivalent) | Three survivors disable the fast path only; the per-byte fallback yields identical results. | Recorded as equivalent; performance, not behaviour, depends on them. |

### Tests

- `bitstream.rs` `component::byte_runs`: `BitReader`, `BitBuf`, `BytesReader`
  (`read_bytes`/`read_into`), the incremental reader (`read_into`), and `BitWriter`/`BytesWriter`
  (`write_bytes`) against the private per-byte loops in all four layouts, start bits 0–17, and
  run lengths through two bytes past the end: equal results, errors, partial fills, cursors.
- `tests/bin_byte_runs.rs` (`macro_`): `Vec<u8>`/`Vec<Octet>` messages against an identical
  message whose element keeps the trait defaults, byte-aligned and behind a `u3`, MSB and LSB:
  equal bytes for lengths 0–255 and equal `BitError` (kind, offset, field) at every truncation.
  Enum-variant runs round-trip.
- `bitstream.rs` Source/Sink default tests: `read_bytes`/`read_into`/`write_bytes` through the
  trait defaults at a 4-bit offset, including EOF. `tests/bin_byte_runs.rs` also decodes runs
  through `StreamBitReader`, matching the slice path's values and truncation positions.
- Hand mutations removing the alignment check from `BitWriter::write_bytes` and
  `BitReader::aligned_run` fail four component tests and both unaligned `macro_` tests.

### Performance

AMD Ryzen Threadripper 7970X, rustc 1.99.0, default features, Criterion defaults, pinned with
`taskset -c 2` (powersave governor), 2026-10-03. Baseline is `origin/main` `ae539d20` plus only
the new `byte_runs_2x255` bench; two runs each, same machine and target directory.

| Case | Baseline A / B | Candidate A / B |
| --- | ---: | ---: |
| `byte_runs_2x255/encode` (513 bytes) | 1.5007 / 1.4912 µs | 97.562 / 96.296 ns |
| `byte_runs_2x255/decode` | 570.25 / 591.86 ns | 29.972 / 31.575 ns |
| `ipv4ish_message/encode` (40-byte tail) | 200.44 / 196.19 ns | 98.856 / 97.907 ns |
| `ipv4ish_message/decode` | 138.45 / 136.57 ns | 46.331 / 46.060 ns |

Not measured: allocation counts, codegen size, the SOCKS session benchmark, and the other
`message_bench` groups.

### Verification (2026-10-03, routine tier)

Passed: `cargo fmt -p bitsandbytes -p bitsandbytes-macros --check`; strict Clippy
(`--all-targets -- -D warnings`) for both crates, default and all features; `cargo test` for
both crates, default and all features; `cargo test -p socks --all-features`; the `nostd-check`
build for `thumbv7em-none-eabi`; `cargo +1.85.0 check -p bitsandbytes --all-features`;
denied-warning `cargo doc` (all features, no default features); `cargo +nightly-2026-06-17
public-api` (additive only, snapshot regenerated); `cargo semver-checks --baseline-rev
ae539d20 --release-type patch --all-features` ("no semver update required"). Independent
reviewer: no correctness findings; two wording fixes applied above.

Pre-commit gate, same day: `cargo mutants -p bitsandbytes -p bitsandbytes-macros --test-package
bitsandbytes --all-features --in-diff <branch diff>` — 54 mutants: 49 caught, 2 unviable, and 3
equivalent survivors (ledger above), after the default-path tests closed five real gaps.
Fuzzing (`cargo +nightly fuzz run`, `-max_len=2048`): `decode` 2,000,000 runs and
`stream_decode` 2,000,000 runs, no crashes. Not run: `scripts/ci-act.sh pre-push`, which
fails locally under Podman 5.8.7; hosted CI is the substitute for this change.

## 17. `From` conversions for `#[bitfield]` (0.8.0, breaking)

`#[bitfield(uN)]` now emits `From<uN> for Struct` and `From<Struct> for uN`, delegating to the
same unvalidated `from_raw`/`to_raw` conversion. A byte-aligned `BitEnum` already has
`num_enum`-parity `From` impls; a bitfield did not, so a consumer that treats both as a
register value (`usdr`'s `Register` trait) needed a macro per register to bridge
`Bitfield::to_raw` and `Into<u8>`. With this, a plain `T: From<u8> + Into<u8>` bound covers
both.

| Finding | Consequence | Disposition |
|---|---|---|
| High, new: the impls land on the user's own type, so this is a blanket-impl-class addition. A crate that already has any of `impl From<uN> for MyBitfield`, `impl From<MyBitfield> for uN`, `impl Into<uN> for MyBitfield`, `impl TryFrom<uN> for MyBitfield` (the usual way to add reserved-bit validation), or `impl TryFrom<MyBitfield> for uN` now fails with E0119; the `TryFrom`/`Into` cases conflict through core's blanket impls. | Source break on upgrade. | Committed as `feat(bitsandbytes)!` with a `BREAKING CHANGE:` footer naming the five impls, so release-plz cuts 0.8.0 for the version group (a plain `feat` only bumps the patch here). The whole `rsl` workspace checks clean, so no in-repo consumer is affected. A validating `TryFrom<u8>` user moves to a named constructor. Reviewer confirmed the `TryFrom<u8>` E0119 in a scratch crate; the others follow from coherence, not compiled. |
| Low, by design: `From<uN>` keeps bits beyond `WIDTH`, like `from_raw`. | `From` cannot be used to validate. | Dual-use rule (section 7); tested in `tests/bitfield_from.rs`. |

No runtime path changes: the impls are two inline field moves, so no benchmark applies.
`public-api.txt` is unaffected (macro output is not bnb's own API).

### Verification (2026-10-08, routine tier)

Passed: `cargo test -p bitsandbytes -p bitsandbytes-macros` (including the trybuild UI suites,
whose snapshots are unchanged; the reviewer also ran `--features mock`); `cargo check
--workspace --all-targets` for the whole `rsl` workspace (no conflicting impls); the
`nostd-check` build for `thumbv7em-none-eabi`; `cargo +1.85.0 check -p bitsandbytes
--all-features`; `cargo semver-checks --baseline-rev origin/main -p bitsandbytes --release-type
minor --all-features` ("no semver update required"). `cargo mutants --in-diff`: 1 mutant,
unviable.

Limitations: semver-checks and cargo-mutants cannot see `quote!` output, so neither says
anything about the new impls; the breaking classification above rests on coherence rules and
the scratch-crate E0119. `scripts/ci-act.sh pre-push` runs before publishing. Independent
reviewer: four blocking findings (semver disposition, conflict list, test module, missing
evidence), all fixed here.

## 18. Review correctness fixes S1–S5 (0.9.0, breaking)

Five findings from the 2026-11 multi-lens review, fixed one per commit. Baseline:
`30c654e8` (`origin/main`).

### Findings ledger

| Finding | Consequence | Disposition and regression check |
|---|---|---|
| S1, High, pre-existing: a `BitEnum` field in a `#[bits(N)]` slot wider than the enum handed the getter bits the enum has no variant for. | The const getter panicked on decoded or `from_raw` input; a primitive in a wider slot silently truncated. | A per-field `const _` assert rejects a slot wider than the type's `Bits::BITS` (`#[view]` fields exempt), and `BitEnum::from_bits` masks its input to the enum width. Source break for any such declaration; the whole `rsl` workspace has none. `tests/ui/bitfield_slot_wider_than_type.rs`, `comprehensive.rs::from_bits_ignores_bits_above_the_enum_width`. |
| S2, Medium, pre-existing: `BufSource` reported a short read after its reader's EOF as `Incomplete`. | Enum magic dispatch stopped instead of falling through to the next variant or catch-all, so the same bytes decoded differently over a slice and over a reader. | Returns `UnexpectedEof` once the reader has hit EOF, as the `ErrorKind` contract states for finite input. Behaviour change: code that detected a clean close by `is_incomplete()` must match `UnexpectedEof` (the `tcp` example is updated). `bin_enum.rs::a_short_tail_at_eof_dispatches_to_the_catch_all_on_every_finite_source` covers slice, `BitReader`, `SeekReader`, `BufSource` and `BitBuf::pull_eof`. `StreamBitReader` still reports `Incomplete` at EOF, documented as its forward-only contract. |
| S3, Medium, pre-existing: a nested `#[bitfield]`/`#[bitflags]` was read and written in the enclosing message's byte order, contradicting the documented declared order. | A `bytes = little` register inside a `#[bin(big)]` message had different bytes than its own `to_bytes`. | `Bits` gains a hidden, defaulted `__BNB_BYTE_ORDER: Option<ByteOrder>` (default `None`); `#[bitfield]` and `#[bitflags]` set it from `bytes = …`, and every typed read and write (`Source::read`, `Sink::write`, the `BitReader`/`BitWriter`/`StreamBitReader` inherent forms) uses it over the cursor's order. Fixing it at the trait rather than in the leaf codec also covers `map`/`try_map`/`bw(map)`, `magic`, `WireLen` and custom codecs, which a first draft missed (review). `BitEnum` and `impl_bits!` types keep the cursor's order. Wire change wherever the two orders differ, including the default `big` inside a `little` message; no in-tree crate nests one (every in-tree bitfield and flag set is big/MSB, and the only little `#[bin]`, netlink, nests none). `bin_byte_order.rs::nested_bitfield_keeps_its_declared_byte_order` (sub-byte offset included) and `::declared_byte_order_holds_through_map_and_lsb_messages`. |
| S4, Low, pre-existing: `decode_all`/`decode_iter` treated the padding bits of a final partial byte as a truncated message. | `T::decode_all(&t.to_bytes()?)` failed for any `T` whose width is not a byte multiple, though `decode_exact` accepted the same bytes. | When fewer than 8 bits are left and the decode fails by running out of them (`UnexpectedEof`), or the unread bits are all zero, iteration ends cleanly; any other failure on a nonzero tail is still reported, and `decode_iter` stays stopped. A tail that decodes is kept, so messages narrower than a byte are not dropped; padding that happens to decode as such a message is indistinguishable, as before. A closed magic enum reports running out of bits as `NoMatchingVariant`, so after one only a zero tail counts as padding (what `to_bytes` writes). Deviates from the plan's "stop on any sub-byte tail", which would drop a genuine final sub-byte message. `entry::decode_all_and_iter_treat_an_undecodable_sub_byte_tail_as_padding`, `entry::decode_all_keeps_a_sub_byte_message_in_the_final_byte`, `bin_assert.rs::a_rejected_sub_byte_tail_is_an_error_not_padding`. |
| Low, pre-existing, out of scope: the generated `__bnb_from_bits` of `#[bitfield]`/`#[bitflags]` stores `raw as Backing` without masking to `WIDTH`, so bits above a narrower-than-backing bitfield's width survive `Bits::from_bits`, against the trait's "higher bits are ignored". | A stray high bit is retained in the backing integer (dual-use retention, not a panic). | Recorded for a follow-up; not changed here. |
| S5, Low, pre-existing: `len * 8` and unchecked position increments overflow on 32-bit targets at 512 MiB. | A wrapped, short bit length (wrong `remaining_bits`/`bit_len`/seek bound) or a debug panic. | `bits_in` saturates byte-to-bit conversion at the last whole byte that fits, used by `remaining_bits`, `seek_to_bit`, `read_slice`, the aligned-run fast path, `BufSource`, `BitBuf::bit_len` and the `net`-gated `BitBuf::read_buffered`; `BitWriter::write_bits`/`write_bytes` and `StreamBitReader::read_bits` return `PositionOverflow` at the bound. `unit::bits_in_saturates_at_the_last_whole_byte_instead_of_wrapping`, `unit::writer_and_stream_positions_fail_at_the_bound_instead_of_wrapping` (host-run; the bound is simulated by setting the cursor). |

Each regression test was confirmed to fail with its fix reverted (S1 by the new compile
error, S2–S4 by reverting the runtime change).

Independent review of the first draft: one blocking finding (S3 fixed only the leaf codec,
so `map`/`magic` paths still used the message's order), fixed by moving the order onto
`Bits`; two should-fix findings (S4 swallowed real rejections on a nonzero tail, and
`decode_iter` could resume after its padding `None`), fixed; the commit's `BREAKING CHANGE`
footer extended to the default-order, `decode_all` and `PositionOverflow` changes.

### Performance

The hot-path changes are one `checked_add` in `BitWriter::write_bits` and one in
`StreamBitReader::read_bits`, a mask in `BitEnum::from_bits`, and every typed read and write
now taking the type's declared order from a trait const (resolved at compile time). Not benchmarked.

### Verification (2026-11-09, release tier)

- `scripts/ci-local.sh release --product bitsandbytes --base 7db0cfb6` on `902a68f8`: all 33
  jobs passed, including the bnb suite, MSRV, `no_std`, package, public API (snapshot
  unchanged), semver-checks (against 0.8.0, `release_type = "patch"`: the public API is
  unchanged; the breaks are behaviour and wire bytes, which it cannot see), the downstream
  SDR/socks/network consumers, and short campaigns of both bnb fuzz targets. A first run
  based on the `bitsandbytes-v0.8.0` tag stopped at commitlint on four older commits by
  other authors.
- `cargo mutants -p bitsandbytes --all-features --in-diff` (the runtime diff): 75 mutants,
  60 caught, 6 unviable, 1 timeout (`read_buffered -> Ok(1)` loops, so detected), 8 missed.
  Three missed mutants, in the LSB branch of `is_padding`, were a real gap, closed by
  `bin_assert.rs::lsb_padding_is_the_high_bits_of_the_final_byte`; a re-run caught all 13
  `is_padding` mutants. The other five are equivalent: `aligned_run -> None` and its bound
  mutants only disable the fast path, whose per-byte fallback returns the same values,
  errors and cursor, and the `read_slice` bound mutant differs only past the 32-bit bound.
- Not run: the S5 tests on a 32-bit target (the i686 build of the test dependencies needs a
  C cross-compiler; the library compiles for i686), mutants in the macro crate (its own
  tests do not exercise the expansion), longer fuzz campaigns, and benchmarks.

## 19. Whole-crate review fixes (0.10.0, breaking)

Findings from the 2026-11-10 five-lens review of 0.9.0 (soundness, performance, API, tests,
style; two rounds, every finding re-checked by an independent verifier with an executed
repro or measurement), fixed one concern per commit. Baseline: `da90c853` (`origin/main`,
the 0.9.0 release).

### Findings ledger

All findings are pre-existing unless marked.

| Finding | Consequence | Disposition and regression check |
|---|---|---|
| R1, High: `decode_all`/`decode_iter` stopped only when a message left the cursor exactly where it started. | A message with a backward `#[br(seek)]` cycled: `decode_iter` yielded without end and `decode_all` pushed until allocation failed. | `<=` is `ErrorKind::NoProgress` at the message's start, as in `decode_vec` and the incremental attempts. This also turns the zero-width case from a silent stop into the same error (breaking; §11.2 already required a message to advance). `bin_seek.rs`, a bounded-work property in `fuzz_roundtrip.rs` (decodes counted, not timed), and the same shape in the `decode` fuzz target. |
| R2, Medium: `BufSource` counted every byte ever read against its cap and never released any. | A long-lived decode loop over one `BufSource` (the shape `examples/tcp.rs` showed) ended in permanent `BufferFull` after `cap` cumulative bytes. | `BufSource::compact()` discards the bytes before the cursor's byte; positions stay stream-absolute, and a seek before the retained bytes is `NotSeekable`. Reclaim is explicit because a later seek may still want the bytes. Accepted limit: with positions absolute, a compacting loop on a 32-bit target ends in `PositionOverflow` after 512 MiB (documented on `compact`). Eight `component::buf_source` tests; a 400-stream differential against `BitReader` in review. |
| R3, Low: `BufSource` surfaced `io::ErrorKind::Interrupted`. | A signal during a read failed the decode, where every other reader retries. | `fill_to` retries. `an_interrupted_read_is_retried`. |
| R4, Medium: enum variant fields were encoded by a hand-copied generator that had drifted from the struct one. | A `br(calc)` variant field was written (decode `[1,5]`, encode `[1,5,6]`); a stored `bw(calc)` was recomputed on verbatim `to_bytes`; `auto_len` never resolved on a variant; `ignore` and `ctx` took the wrong precedence. | One generator, parameterised by where the field values live (`WriteSite`). Struct expansion is byte-identical before and after over 70 expanded files. `bin_enum_fields.rs`, one test per directive. A stored `bw(calc)` on a variant is now a compile error (R8), since an enum has no canonical encode to recompute it. |
| R5, Medium: `auto_len = bytes(source)` measured the source with a bare `bit_encode`. | The length disagreed with the body whenever the source had `write_with`, `bw(map)` or `ctx`: a `u32` behind `leb128::write` reported 4 bytes for a 1-byte body. | The probe replays the source's own write statement against a scratch sink, first binding the `temp` values declared between the length and the source, and replaying a `count_prefix`. The recorded limits of §11.3 (a default-layout, scratchless, offset-zero probe) are unchanged. `bin_wire_len.rs`. |
| R6, Medium: `BitWriter`/`BytesWriter` were `!Send`, through `Option<Box<dyn Any>>` scratch. | A writer could not move into a thread or task. | `with_scratch` takes `Box<dyn Any + Send>` (breaking for a non-`Send` scratch). A unit test asserts both writers are `Send`. |
| R7, Medium: canonical encode is per message, but the docs said "always spec-compliant". | A parent reported `is_canonical()` while a nested message or `Vec` element was not, and wrote it as stored. | Documented as the contract (§5.2), pinned by `bin_canonical.rs`; deep canonicalization is a ROADMAP §C item. No codegen change (owner decision, 2026-11-10). |
| R8, Low to Medium: the macros accepted input they then mishandled. | Conflicting or repeated field directives (one silently won); container options ignored on enums and mapped structs; `validate` not compiling under `no_std`; a `read_only` type demanding write-side impls; two proc-macro panics; generated names `S`/`K`/`__e` capturing user items; `bool`/`u8` through a `$t:ty` fragment not recognised; field names colliding with generated methods. | One commit each. A shape is rejected only if, at 0.9.0, one of its directives was silently ignored or the output was wrong; shapes where everything took effect stay accepted. New rejections are spanned errors with trybuild snapshots and `BREAKING CHANGE` footers, and the conflict table has a test that drives every pair of its 23 directives through the attribute parser. The review's "duplicate variant magics" finding was withdrawn: variants that share a magic are a legitimate shape (a request and a response told apart by direction), so it is documented (`decode` takes the first declared match, the rest through `decode_as_<variant>`) and tested, not rejected. |
| R9, Medium, tests: behaviour-changing mutants left the suite green. | `pad_after`/`align_after` on decode, the bitfield setter mask, `decode_exact_with` exactness, the `StreamBitReader` LSB carry, `BufSource` cap arithmetic, `SourceReader` EOF mapping and `LimitedSource` bounds were unpinned. | Tests added for each; every listed mutation was applied by hand and seen to fail. A fixed-seed parity property now runs one read sequence through all six readers in all four layouts. |
| R10, Low: the unpack benchmarks folded their input. | The documented "unpack ~190 ps" was the harness floor; the real figure is ~430 ps (§10). The comparison's conclusion held. | `black_box` moved inside the measured closure for every compared implementation; §10 and `AGENTS.md` corrected. |
| R12, Low to Medium, API: `UInt<T, N>` converted only from its own backing type. | Callers cast with `as` first and truncated silently: `u12::try_new(65541usize as u16)` is `Ok(5)`. | `TryFrom` from all twelve primitive integers (55 new impls from one macro), `WidthError::Negative`, and the hex, octal and binary formatters. The set has to be complete: with more than one source but no `i32`, a bare literal stops compiling. The cost, accepted by the owner on 2026-11-11 with the list in the commit footer: an *untyped* argument is now evaluated as signed `i32`, as for the primitive integers, so `!`, `>>`, `/`, `%` and bit 31 change meaning (`u12::try_from(!0 >> 4)` was `Ok(4095)` and is `Err(Negative)`). No call site in this workspace or in libsdr is affected. Checked against a reference over 5.1M conversions in review. |
| R11, Low: documentation false at HEAD and dead code. | Wrong option lists, a backwards prefix-order rule in the dispatch guide, dead `::bnb::` rustdoc links in consumer docs, references to roadmap phases that no longer exist; `peek_bytes` and a `__private` export with no callers; three duplicated helpers. | Corrected or removed. `peek_bytes` was last emitted by the 0.4 macros, which the runtime's exact macro pin cannot pair with. |

Recorded, not changed (owner decisions, 2026-11-10): unbounded recursion on self-containing
types aborts on stack overflow (§11.4 now says so, with the caller-side bound; a depth
budget is a ROADMAP §C item); `BinCodec` sets no maximum frame size (documented);
`ErrorKind::BadMagic` stays unboxed (measurement in ROADMAP §C).

### Independent review

Each runtime or macro change was reviewed by a separate reviewer before it joined the
series, and these reviews changed the result:

- The first `auto_len` probe rewrite rejected a length-type-value layout (`len`, a `temp`
  tag, a body whose `ctx` reads the tag) that compiled and encoded correctly in 0.9.0. The
  probe now binds those temps itself; the layout is a byte-pinned test in structs and variants.
- The first byte-wise read (below) made fields of one to seven bits 1.15 to 1.6 times slower
  to decode, on a shape no benchmark covered. Fixed with the narrow inline path; sub-byte
  benchmarks in both bit orders now exist.
- `compact()` had two unpinned lines (the read-ahead bound and the EOF remainder after
  compaction); both now have tests that fail on the mutation.
- The diagnostics batch was reviewed twice by compiling every newly rejected shape against
  the 0.9.0 macros. The first draft had lost one rejection (`br(calc)` with `ignore`) and
  rejected shapes that worked: a stored `restore_position` peek or an `ignore` field with
  `bw(calc)`, `reserved` with `temp` and `calc`, `no_builder` where there is no builder, and
  variants sharing a magic. All were restored; the series was rebuilt without the
  duplicate-magic commit.
- The `UInt` review found the silent `i32` evaluation of untyped arguments, which the first
  footer described only for shifts on wide backings.
- The stream-scratch test passed with the scratch unused; it now asserts the bytes are there.

### Performance

The 0.9.0 review measured two costs in the read primitive: a read or write that started or
ended inside a byte ran one loop iteration per bit (an unaligned 64-bit read took 54 ns
against 5 ns aligned), and `read_bits` was never inlined into generated decoders, so a
field's constant width was not folded. §18 had also shipped its hot-path changes unmeasured:
the buffered gate probe read 405.22 instructions per message at `da90c853` against 392.22 at
0.8.0 on the same toolchain (the 378 recorded in §13 was Rust 1.98.0).

The fix is one commit. Reads and writes work per touched byte (§10); the slice reader's
bounds check, aligned path and narrow path are forced inline; the wide unaligned paths and
the error constructors are out of line.

Environment: Rust 1.99.1, Linux x86_64, Threadripper 7970X, system allocator, default bnb
features, CPU affinity 61. Baseline and candidate bench executables were built in separate
target directories from the parent commit and the candidate, and run alternately three
times each with Criterion's defaults, 1 s warm-up and 3 s measurement; the figure is the
lowest of the three medians. Other builds were running on other cores during some runs.

| Benchmark | Parent | Candidate | Ratio |
|---|---:|---:|---:|
| `dmr_frame/decode` | 242.6 ns | 34.0 ns | 0.14 |
| `dmr_frame/encode` | 290.8 ns | 140.0 ns | 0.48 |
| `sub_byte/msb_decode` (new) | 66.8 ns | 32.6 ns | 0.49 |
| `sub_byte/lsb_decode` (new) | 61.6 ns | 31.3 ns | 0.51 |
| `sub_byte/msb_encode` (new) | 120.3 ns | 94.3 ns | 0.78 |
| `ipv4ish_message/decode` | 49.8 ns | 34.3 ns | 0.69 |
| `byte_aligned/decode` | 22.8 ns | 17.2 ns | 0.76 |
| `enum_dispatch/integer_valid` | 11.1 ns | 8.9 ns | 0.80 |
| `enum_dispatch/mixed_1_in_16_invalid` | 173.4 ns | 129.9 ns | 0.75 |
| `enum_dispatch/buffered_chunk_1` | 1359.9 ns | 1086.4 ns | 0.80 |
| `enum_dispatch/buffered_chunk_5` | 265.5 ns | 187.5 ns | 0.71 |
| `enum_dispatch/buffered_chunk_80` | 205.5 ns | 139.3 ns | 0.68 |
| `incremental_envelope/1024/64` (slowest ratio) | 236.5 ns | 247.4 ns | 1.05 |

No other benchmark in the two suites is slower than 1.04.

**Accepted regressions (owner approval, 2026-11-11).** The independent re-review ran eight
alternating pairs of the four `incremental_envelope` cases that read slower and found them
repeatable: 1024/64 235.5 → 239.9 ns (+4.4 ns per envelope, slower in 8 of 8 pairs),
1024/1500 46.9 → 49.8 ns (+2.9 ns, 6 of 8), 16384/64 3359 → 3453 ns (+94 ns, 7 of 8),
16384/1500 515.3 → 525.1 ns (+9.8 ns, 5 of 8). The cost arrived with the narrow inline path;
neither the reviewer nor the author found a variant that removed it without giving up more
elsewhere. Separately, a hand-written loop that calls the primitives directly with a
constant width (no in-tree code, no committed benchmark) no longer folds as far as it did
where the old `#[inline]` read happened to inline: aligned `read_bits(8/16/32)` 0.66 → 0.92
ns, `read_bits(1/3/4)` 0.18 → about 0.9 ns, `write_bits(1/3/4)` 1.9 → 3.7 ns per call; the
same loops in a standalone function are faster than before (2.20 → 0.74 ns for the aligned
read). Every shape generated by the macros is faster. A later presizing change measured a
suite whose code was byte-identical in both builds at 0.95 to 1.05, which is the resolution
of this method. Revisiting the envelope path is a ROADMAP §E item.

| Fixed-work probe, three alternating runs, instructions per message | Parent | Candidate |
|---|---:|---:|
| `BNB_DISPATCH_PROBE=buffered` (16 messages per drain, 5-byte chunks) | 405.22 | 270.22 |
| with `BNB_DISPATCH_PROBE_CHUNK=1` (four of five attempts fail) | 1774.22 | 1243.22 |
| with `BNB_DISPATCH_PROBE_CHUNK=80` (whole frames) | 283.91 | 148.91 |

Counts were identical to the hundredth across the three runs. Cycles per message fell from
79.7 to 57.5, 416 to 343 and 61.7 to 42.7. Release `.text` of the two bench executables
moved by +236 and +6,416 bytes (3.38 MB and 3.32 MB).

Variants built and rejected, each measured the same way:

- Both general paths inline in `extract_bits`: byte-aligned decode 8% slower, `byte_runs`
  decode 7% slower. The larger body changed how the aligned path was compiled.
- The read left `#[inline]` with the wide path outlined: the optimizer inlined it in one
  build (byte-aligned decode 0.56) and not in the next after an unrelated edit.
- `#[inline(never)]` on the read, to pin the old shape: `ipv4ish` decode 22% slower.
- The wide path out of line with no narrow path: fields of one to seven bits decoded 1.45
  times slower (found in review; the suite had no such shape).
- The incremental reader's wrapper inline but still converting the slice reader's
  end-of-input error: one-byte-chunk buffered decode 1.24 times slower with *fewer*
  instructions. `perf annotate` put 86% of the decoder's samples on the first wide load of
  the 80-byte error after the conversion (a store-forwarding stall). Building the
  `Incomplete` error directly removed it.

The two `#[inline(always)]` attributes are the first in the crate. They sit on a function
whose body is two compares, two short loops and two calls, and are justified by the
flip-flopping above rather than by a general policy; libsdr's measurement that blanket
inline hints made its decoders slower is consistent with that, and no generated code
carries one.

**Follow-ups on the same gate** (one commit each, parent and candidate built separately,
three alternating pinned rounds; an independent reviewer re-measured each):

| Change | Benchmark | Parent | Candidate |
|---|---|---:|---:|
| A fixed-length `#[bin]` message's `to_bytes` reserves its exact size | `fixed_44/encode` | 83.3 ns | 35.1 ns |
| Byte-aligned `u16`/`u32`/`u64`/`u128` runs convert in one pass (§16's hooks, extended) | `counted_u16_1024/decode` | 1953 ns | 68.5 ns |
| | `counted_u16_1024/encode` | 1688 ns | 99.9 ns |
| | `counted_u32_256/decode` | 586 ns | 51.2 ns |
| | `counted_u32_256/encode` | 739 ns | 88.6 ns |
| `MessageStream` keeps one read scratch instead of zeroing 4 KiB per call | `stream_envelope/16x40/704` | 992 ns | 629 ns |

The hand-written reference loops beside the run benchmarks read 311 ns (`u16` decode) and 446
ns (`u16` encode). Decode still proves the bytes are present before it allocates: a count of
`usize::MAX` over 40 bytes allocated at most 384 bytes, before and after. The probe counts
did not move in any of the three. The bulk path was compared with the element loop over 3.3M
cases (every source and sink, four layouts, every truncation point, x86_64 and 32-bit) with
identical values, errors and cursors. Mixed-width magic dispatch, measured only: the eighth
of eight variants costs 2.7 times the first (25.9 against 9.8 ns), down from 4.5 to 6.3 times
at 0.9.0, with no change to the dispatch.

**What this harness can resolve.** Benchmarks whose code a commit did not touch moved by up
to 11% between its parent and candidate builds. In every such case the benchmarked path was
instruction-identical in both, and the gap disappeared when both were built with
`-C llvm-args=-align-all-functions=6`. Adding a benchmark to a bench executable also moved
its neighbours (a new `MessageStream` case doubled `incremental_envelope/*/1` in parent and
candidate alike, and a buffered case moved the probe by 22 instructions). Treat a difference
under about 5% between two builds as placement until the disassembly or the fixed-work
probe says otherwise, and check the probe and the neighbouring benchmarks before and after
adding one.

### Structure

The codec macro (`bnb-macros/src/bitstream/`) is split into modules by responsibility, as a
pure move: 1,044 macro expansions across the workspace produce byte-identical token streams
before and after, and no trybuild snapshot changed. The slice entry points that four
front-ends pasted come from one generator.

The same split of the runtime (`bnb/src/bitstream.rs`, about 6,800 lines) was built and
**not taken**. It passed every functional check, but rustc partitions a crate's code by
module, and after the move the incremental pull path (`BitBuf::pull` → `try_pull_with` →
`decode_attempt`) was compiled differently in a consumer: with function alignment forced on
both builds, `incremental_envelope` read 12% to 23% slower across ten cases (1024/1:
11.9 → 14.5 µs), and a second layout that put the attempt helper beside `BitBuf` read up to
2.3 times slower. The hot functions of the dispatch probe were instruction-identical, so the
probe alone would not have caught it. The split needs `#[inline]` decisions on that path
measured one at a time, as its own change (ROADMAP §E).

### Verification

Run on the series as it stands before this record (2026-11-11, rustc 1.99.1):

- `cargo fmt --all --check`; `cargo clippy -p bitsandbytes -p bitsandbytes-macros
  --all-targets --all-features -- -D warnings`: clean.
- `cargo test -p bitsandbytes -p bitsandbytes-macros --all-features`: 842 passed, 0 failed,
  11 ignored, including trybuild; no snapshot changed except those written for new errors.
- `cargo check --workspace --all-targets --locked`: clean. Consumer suites (`dns`, `socks`,
  `netlink`, `tcp`, `ip`, `arp`, `rtltcp`, `asn1`): 182 passed.
- `cargo +1.85.0 check` on both crates; the bare-metal `nostd-check` build; rustdoc with
  warnings denied; `cargo public-api` equal to the snapshot (additions: `BufSource::compact`,
  the `BinCodec` impls, the `UInt` conversions and formatters, `WidthError::Negative`;
  changed: `BitWriter::with_scratch`, both writers now `Send`).
- Every regression test named above was seen to fail with its fix reverted or mutated by
  hand. `cargo mutants` was not re-run over the diff.
- `scripts/ci-local.sh pre-push` on `f3ba0b34`: commitlint over the series, policy, and the
  16 routine check jobs passed (the workspace job ran 1,484 tests).
- `scripts/ci-local.sh release --product bitsandbytes` on `f3ba0b34`: 31 jobs passed
  (MSRV, bare-metal `no_std`, public API, packaging, the bnb and socks suites, both bnb fuzz
  targets, the SDR and network consumers); semver-checks failed in patch mode against 0.8.0
  on `BinCodec` gaining `Copy`. The gate now runs against 0.9.0 in major mode with the
  reviewed `api-delta-0.10-*.txt` (nothing removed), and passes.
- libsdr at `32467d37` against `f3ba0b34`: no source change needed, identical test outcomes,
  standalone Full LC decode 12–18% faster, receiver benchmark unchanged (ROADMAP, 0.10.0).
- Not run: a 32-bit run of the unit suite (the library and a probe of the pointer-sized
  conversions build and run for `i686-unknown-linux-musl`), `cargo mutants`, and fuzz
  campaigns longer than the release tier's.

## Named context dispatch trial (2026-11-09)

Baseline: `eca60e6b9868165c5658bdcb2a7ca1a51f9ca2fa`; branch
`feat/named-context-tags`. The libsdr DMR trial is the representative consumer.
`ctx { child = source }` aliases identifiers without conflating parent scope
and child member names. `tag(fid = ..., code = ...)` compares typed constants
conjunctively, in declaration order, with the same selector set on every tagged
variant. Integer literals have a const round-trip range check. Named catch-all
fields capture selectors without reading/writing bits; generated per-selector
accessors let parents reconstruct header fields. This removes libsdr's packed
FID/opcode dispatch key without introducing another wire schema.

The existing single-tag and magic paths remain available. Mixing named tags
with those dispatch forms is rejected; enum-level prefixes and typed fallbacks
remain supported. No runtime library API or dependency feature changes.

Independent reviewer findings in `bnb-macros/src/bitstream.rs`, all introduced
in this candidate and fixed before final review:

| Severity | Location / consequence | Disposition and regression evidence |
| --- | --- | --- |
| Blocking | `NamedTag::typed_value`: suffixed integer literals failed type checking | Normalize to `u128`; aliased `u6` maximum `0x3fu8` succeeds; UI case rejects `0x40` |
| Blocking | `tokens_mention`: alias target label falsely required encode context | Ignore ordinary field labels; `DecodeOnlyParent` encodes without irrelevant context |
| Blocking | Label filtering hid references in opaque macro arguments | Preserve conservative recursive scanning inside macro groups; `MacroContext` covers nested arguments |

Validation uses host Rust/Cargo 1.99.1, default features, both affected packages:
focused named-tag tests (7 passed), the full package tests (including UI and
Rustdoc), strict all-target Clippy, and formatting. libsdr uses its pinned
1.97.1 standard gate and separately exercises zero-allocation DMR reception.
The reviewer approved the final fixes by inspection; no benchmark improvement
is claimed. The container pre-push gate later passed for `580d8f20`, and the
owner authorized merging and releasing it with 0.9.0 on 2026-11-10.

A second independent review (2026-11-10) found, and the follow-up commit fixed:

| Severity | Location / consequence | Disposition and regression evidence |
| --- | --- | --- |
| Should-fix | Enum encode's context scan included match patterns, so a stored field named like a ctx param (a named-tag catch-all's captured selector, a typed fallback or catch-all field) looked like a context read: such an enum lost plain `to_bytes()`, and adding a catch-all changed the encode API | Owner decision (2026-11-10): fix it in 0.9.0 rather than a later breaking release. A stored field the arm's pattern binds shadows the param for that arm and is excluded from the scan; a `temp` field, bound later by its own `calc`, is not (`temp_calc_reading_ctx_keeps_encode_with`). The plain `bit_encode` allows the unused captured bindings. Breaking for existing enums of that shape: they gain a plain `BitEncode` and `to_bytes`, and lose the inherent `encode_with`/`to_bytes_with`. In-tree, DNS `RData` (`Custom { rtype, .. }`) did, so its hand-written context-free `BitEncode` (E0119 against the generated one) is removed; the generated impl does the same thing |
| Should-fix | Variant-arm alias sources (ctx param, stored sibling) untested | `variant_alias_sources_resolve_from_ctx_and_siblings` |
| Should-fix (docs) | Integer literals need a directly named `Bits` type; aliases of primitives, signed integers and `usize` fail with an unspanned error | Restriction and the `(7)` workaround (type-checked, unlike a truncating `as`) documented in `guide::dispatch`; accepting bare literals is additive and left for 0.9.x |
| Nit | Selector accessors can clash with user methods | Documented in `guide::dispatch` |
| Nit, recorded | Identical condition sets on two variants compile silently; the overflow assert is reported at the enum attribute; no UI cases for magic + named tags, a tuple catch-all, empty `tag()`, or a duplicate `tag` | Follow-ups, not changed |
| Note | `tokens_mention` now ignores `name:` labels, so a ctx struct whose only mention was a label switches from `encode_with` to a plain `BitEncode` | Listed in the 0.9.0 breaking-change notes |

## 20. Decode depth budget (0.10.1)

A message type that contains itself decoded by native recursion to a depth the input chose,
so hostile input overflowed the stack: an abort no caller could catch (§11.4, reproduced in
the 0.9.0 review and again here as the failing test). Generated decoders now fail with
`ErrorKind::TooDeep` once more than `MAX_DECODE_DEPTH` (128) non-empty element loops are
open on one source.

### Contract and design (owner decisions, 2026-11-11, after a skeptic review)

- **What counts.** Generated code can recurse only through a counted `Vec`: there is no
  `BitDecode` for `Box` or `Option` (`#[br(if)]` reads an `Option` field without one), and a
  directly self-containing field has infinite size. So the two element loops count — the
  default `BitDecode::decode_vec` and the context-taking loop the macro emits — once per
  non-empty `Vec`, not per element or per message. That also bounds indirect recursion
  (A → B → A), which the macro cannot see, and charges nothing to a message with no `Vec` of
  messages (the probe's `NumericDispatch`, `dmr_frame`). Leaves opt out rather than messages
  opting in, so a missed message generator cannot reopen the abort: every leaf `BitDecode`
  (integers, `bool`, IP addresses, `UInt`, `WireLen`, and the impl `#[bitfield]`/`BitEnum`/
  `#[bitflags]` emit) overrides `decode_vec` with the uncharged loop, and the integer bulk
  path falls back to it. A first version charged the shared loop, so a leaf run counted only
  when the bulk path declined it: the same bytes at the limit decoded from a slice and failed
  from a `StreamBitReader` (found in review). A hand-written impl that keeps the default is
  charged, which is conservative. Recursion inside a `parse_with` codec or a hand-written
  `bit_decode` is that code's to bound.
- **Where the count lives.** A required, hidden `Source::__bnb_depth(&mut self) -> &mut u16`.
  `Source` is sealed, so a required method breaks no one, and being required makes every
  source decide (owned count, or forwarding: `LimitedSource` and `IncrementalReader` forward
  to what they wrap). `__private::nested_vec` checks, increments, runs the loop, and restores
  the previous count whatever the loop returns, so a source that outlives a failed decode
  (`StreamBitReader`, `BufSource`, a `BitBuf` used directly) is not charged for it. An
  incremental attempt builds a fresh reader, so its count starts at zero.
- **Limit.** `pub const MAX_DECODE_DEPTH: u32 = 128` (serde_json's default), no setter in
  0.10.x; a knob can be added later without breaking anything. It counts levels, not stack
  bytes: 128 levels of the test's `Node` decode on a 2 MiB thread stack (Tokio's worker
  default) in a debug build, but the reviewer's `Fat` (a 4 KiB array inline) overflows that
  stack at the limit. The docs say so and keep the caller-side advice (stack for 128 frames,
  or a frame cap) for large types.
- **Name.** `ErrorKind::TooDeep`, a unit variant beside `TooWide`, chosen with 0.11's
  error-type decisions in view (ROADMAP). `ErrorKind` stays 48 bytes and `BitError` 80.
  Reported at the over-limit `Vec`'s first element, with the field name.

### Tests

`tests/bin_depth.rs`, on 2 MiB threads: the limit decodes, one past it is `TooDeep` at the
right bit, a 1 MiB chain errors instead of aborting (it aborted with `SIGABRT` before the
change), width does not count, the ctx loop and indirect recursion are bounded, a reused
`BitBuf` decodes a chain at the limit after a too-deep failure, byte-at-a-time `pull`
attempts do not add up, a leaf run at the limit decodes from both a slice and a stream, and
enum-payload recursion is bounded. Hand mutations, each caught: no restore (1 test fails), `>=` → `>`
(4), no empty-`Vec` skip (5), no increment (the binary aborts), the default `decode_vec`
uncharged (aborts), the integer bulk fallback charged (1). The `decode` fuzz target gained
the recursive shape with an exact oracle (129 leading nonzero bytes are `TooDeep`) and a
`recursive_deep` seed; 2,000,000 runs, no findings.

### Independent review (2026-11-11)

| Severity | Finding | Disposition |
|---|---|---|
| Blocking | The docs promised `TooDeep` instead of a stack overflow and dropped the caller-side advice, but the limit counts levels: `Fat` (a 4 KiB array inline) overflows a 2 MiB debug stack at 128 levels (run by the reviewer). | Docs (`MAX_DECODE_DEPTH`, `guide::io`, §11.4) state that the limit counts levels, not stack bytes, and keep the advice for large types. |
| Should-fix | Leaf runs were charged only when the integer bulk path declined them, so the same bytes at the limit decoded from a slice and failed from a `StreamBitReader` (run by the reviewer). | Leaves opt out (above). `leaf_runs_are_not_charged_on_any_source`; charging the bulk fallback again fails it. |
| Should-fix | The roadmap tick for the baseline reset belonged to another concern. | Its own commit. |
| Nit | `incremental_attempts_restart_the_count` cannot fail on a missing restore (each attempt builds a fresh reader). | Renamed `incremental_attempts_do_not_share_a_count`; the restore is pinned by the reused-`BitBuf` test. |
| Nit | No enum-payload recursion test. | `enum_payload_recursion_is_bounded`. |
| Nit | The ctx loop's outer `in_field` is redundant for `NoProgress`. | Kept: it names the field on `TooDeep`, and `in_field` keeps the innermost. |

### Performance (gate against the `bitsandbytes-v0.10.0` tag)

Environment as §19: Rust 1.99.1, Threadripper 7970X, `taskset -c 61`, system allocator,
default features (`net` for `stream_envelope`), one target directory per tree, three
alternating rounds, 1 s warm-up, 3 s measurement, lowest of three medians.

- **Probe, unchanged:** 270.22 / 1243.22 / 148.91 instructions per message (chunk 5 / 1 /
  80), identical before and after. A `u32` count first cost exactly one instruction per
  attempt (271.22 / 1248.22 / 149.91): zeroing it was a separate store next to the order
  bytes' `movw`. A `u16` sits beside the two one-byte order fields and is zeroed by the same
  store; a `u8` brought the extra instruction back.
- **Suites** (final code, after review): in the plain builds, ten of 61 benchmarks fall
  outside 0.95–1.05. Each of the ten was then rerun in builds made with
  `-C llvm-args=-align-all-functions=6` (§19's placement check; the `net` build for
  `stream_envelope`), along with the dispatch, sub-byte, DMR, counted-run, byte-run,
  `fixed_44` and 1024-byte envelope groups. Every one reads within ±5% there
  (`stream_envelope/*/1`, 1.08–1.14 in the plain builds, reads 1.003–1.036 with overlapping
  ranges; the `*_hand` reference loops, which call no bnb code, read 1.30 plain and 1.00
  aligned), except:

| Benchmark (aligned builds) | 0.10.0 | Candidate | Ratio |
|---|---:|---:|---:|
| `enum_dispatch/bytes_invalid` | 35.05 ns | 37.73 ns | 1.076 |
| `enum_dispatch/integer_invalid` | 30.61 ns | 28.00 ns | 0.915 |

**Accepted regression (owner approval, 2026-11-11).** `bytes_invalid` is repeatable (ranges
do not overlap). The disassembly of its inlined loop shows why: building the `BitReader` with
the new field changes register allocation, so one argument is spilled and reloaded per
iteration and the frame grows by 16 bytes. `integer_invalid` moved the other way in the same
builds. No variant tried (a `u8` count) removed it without costing the probe. Approved at
+6% (34.85 → 36.95 ns, first version); the final code reads +7.6%. It is a failure path, the
trade the root `AGENTS.md` happy-path rule prefers.

## 21. `BinCodec` frame cap (0.10.1)

`BinCodec` took whatever a message's grammar asked for, so a peer announcing a huge `count`
made `Framed` buffer without limit (recorded as an owner decision in §19 and documented;
the workaround was an outer `LengthDelimitedCodec` or a hand-written wrapping `Decoder`).

### Contract

- `BinCodec::with_max_frame(bytes)`; `new()` is `with_max_frame(usize::MAX)`, so existing
  code is unchanged. The codec stays `Copy` and keeps no state between calls: the cap is
  configuration.
- A message is over the cap when it spans more than `bytes`. `decode` reports that as soon
  as `bytes` are buffered and the attempt is still `Incomplete`, and both `decode` and
  `decode_eof` report it when a complete message decodes past the cap, so one frame gets
  the same answer however the input was split into reads. Bytes after the message do not
  count.
- The error reuses `ErrorKind::BufferFull { cap }` (a retained-bytes cap, as for
  `BufSource`), positioned at `cap * 8` (saturating) and wrapped in `InvalidData`; no new
  variant ahead of the 0.11 error-type release. The buffer is left unchanged, as for every
  other `BinCodec` error.
- Other errors keep priority: the cap is checked only on success and when the attempt ran
  out of input (`Incomplete`, or `UnexpectedEof`/`IncompleteAtEof` at EOF), so a bad magic
  inside the cap is still `BadMagic`. The EOF case matters for `UdpFramed`, which calls only
  `decode_eof`: a datagram as long as the cap that does not hold the message reports the
  cap, as `Framed` would. One imprecision, accepted: a truncated bounded region inside such a
  datagram also reads as over the cap.
- The cap bounds buffered wire bytes, not what decoding allocates.

### Tests and evidence

Ten component tests in `framing.rs` (written first, failing to compile without the method):
exactly the cap decodes; incomplete below the cap waits; incomplete at the cap fails with
the buffer kept; an oversize frame fails whole or byte by byte; EOF truncation at and below
the cap; an oversize frame at EOF; `BadMagic` keeps its kind at and past the cap; a 12-bit
message needs a cap of 2 (its padded final byte counts); trailing bytes do not count; the
default has no cap; `Debug` with and without a cap. Hand mutants killed: `<=`→`<` and
`true` on the success guard, `>=`→`>` on the ran-out check, position `0`, `end / 8` (the
sub-byte test), dropping the EOF arm of the ran-out check, and checking the cap before the
error kind (the `BadMagic` test). `cargo public-api` regenerated; `cargo semver-checks`
against `bitsandbytes-v0.10.0` passes as a patch in default, no-default and all-features
modes. Independent review: no blocking findings; its should-fixes (the `UdpFramed` EOF
path, the sub-byte, EOF and priority tests, this section, the ROADMAP) are applied. A
`BinCodec` fuzz shape stays with ROADMAP 0.10.x item 5.

### Performance

The success path adds one compare of the end offset it already computes; the over-cap
error is built out of line (`#[cold]`). No benchmark or probe drives `BinCodec` (the probe
measures `BitBuf`), so none was rerun; the decode path the suites measure is unchanged.

## 22. Happy-path performance review (2026-11-12)

A review of the runtime and the generated codec at `839ccecf` through one lens, under the
root `AGENTS.md` rule that prefers decoding valid input faster over failing faster. Three
reviewers (slice decode core, generated code, streaming and transports), then two
independent reviewers told to refute every finding. Nothing was changed; the work it led to
is ROADMAP 0.10.x item 4.

**Status of the numbers.** Measured on scratch copies, Threadripper 7970X, Rust 1.99.1,
default features, pinned, with other reviewers running at the same time. They size the
work; they are not gate measurements. Each phase re-measures against `bitsandbytes-v0.10.0`
by §19's method. "ins" is user instructions per decode from a fixed-work probe.

### Main result

Most of the lost speed is an inlining boundary, not failure-path protection. §19 pinned
`read_bits` inline, one level below `Source::read`, the function generated decoders call;
`read` stays out of line (18 call sites in `message_bench`, 24 in `bitstream_bench`), so
each field pays a call, a `Result` returned through memory and runtime order branches. §10's
statement that these paths are forced inline into generated decoders is not true of `read`.

### Confirmed

| # | Where | Cost | Prototype result | Who pays |
|---|---|---|---|---|
| 1 | `Source::read` not inlined (`bitstream.rs`) | call + 80-byte `Result` per field | `fixed_44/decode` 20.7 → 9.05 ns, `ipv4ish_message/decode` 36.0 → 19.1, `byte_aligned/decode` 15.1 → 9.06, `sub_byte/msb_decode` 32.9 → 22.9 (with row 3) | buffered probe 270.22 / 1243.22 / 148.91 → 341.22 / 1490.22 / 219.91 as built; 271.22 / 1260.22 / 149.91 with `IncrementalReader::read_bits` pinned too |
| 2 | `decode_attempt` moves the reader it just built | two store-forwarding stalls where the attempt is not inlined | `stream_envelope/16x40/704` 662 → 529 ns; `BinCodec::decode`, 68-byte frame, 33.0 → 27.6 ns; failing `pull` 107.6 → 74.6 cycles | nothing |
| 3 | `extract_unaligned_bits` folds 16 bytes singly | 162 ins per wide read | `dmr_frame/decode` 31.0 → 16.2 ns; 66,304 cases equal | nothing |
| 4 | Fixed-length messages check per field | ~55 ins per scalar field | `fixed_44/decode` 20.0 → 7.4 ns, `sub_byte/msb_decode` 33 → 11.6 | a lone `Incomplete` attempt up to +10% cycles; see below |
| 5 | `net::read_limit`, `shortfall` out of line | 11.6% of `stream_envelope/16384/1` | `1024/1` 6.42 → ~5.4 µs, `16384/1` 100.8 → 85 µs; `incremental_envelope` flat | nothing |
| 6 | Transport encode allocates, then copies | `BinCodec::encode` 70.4 ns against `to_bytes` 34.8 ns (44 bytes); 467 against 210 ns (16 KiB) | not built here; `BinCodec` done in phase 3, below | nothing |
| 7 | `decode_exact` copies the message on success | 11–15 cycles per call, in three placements | not built | nothing |
| 8 | Stream readers read one byte per `read_exact` | 16 KiB payload 58–130 µs against 0.155 µs from a slice | not built | error behaviour, see below |
| 9 | The probe covers one enum through `BitBuf` with `decode_attempt` inlined | read identically across row 2's 20% gain and a 1.37× regression | — | — |

Row 4 as prototyped is not mergeable: row 1 already gives about 80% of its byte-aligned
gain (sub-byte is what remains: 657 → 160 ins in that reviewer's probe executable, where
the base reads lower than the 854 of phase 0's); it turns the public `FixedBitLen` from a
sizing hint into a decode-correctness contract; it puts inline hints in generated code,
which §19 says none carries; and `ipv4ish_message/decode` read slower (34.9 → 38.4 ns).

Row 8 is the one clear case of a failure path protected at the happy path's expense. The
reviewer's claim that bulk reads keep errors identical was refuted: `PositionOverflow`
mid-run differs, and the amount a third-party reader consumes on a failed `read_exact(n)`
is unspecified. It needs a differential test before any change.

Smaller, confirmed as costs with unbuilt fixes: `LimitedSource` 3.6× an unwindowed read
(no consumer outside bnb); `BytesReader::read_bits` 1.84× `BitReader` (none); byte-string
magic fallthrough about 72 ins per missed variant (none; §19 has it as measured only);
per-field error decoration is about 750 of the 1,010 bytes of one five-field decoder (size,
not executed instructions); `assert`, range and prefix rejections allocate a `String`
(failure only, API-visible, 0.11); transport reads are capped at the 4 KiB scratch (the
floor without the copy is not reachable without zeroing, which §19 measured).

### Hypotheses, to research before scheduling

- An inline aligned fast path on `IncrementalReader::read_bytes`/`read_into`. The stall is
  in the disassembly; reading its share of samples as its cost was unsound (the function was
  instruction-identical in two builds that timed 172 and 404 ns).
- `BitBuf` compaction: O(cap) per message when a bounded buffer is kept full; the unbounded
  case was not measured and the proposed policy contradicts the type's documentation.
- A start capacity for variable-length `encode_to_vec`.
- The fixes for rows 6–8 and the smaller items.

### Refuted, and corrections to earlier records

- §19 does not gate on `*_invalid` benchmarks: its sentence is a result, no variant was
  rejected over one, and the tiers are already in the root `AGENTS.md`. Phase 0 writes them
  into the gate text.
- "Copies of the 80-byte error" (ROADMAP §E) is, on the failing attempt, one stalled load of
  the error's last 16-byte lane; boxing `BadMagic` does not touch the success-path stalls.
- Protections worth keeping: no pre-allocation from an untrusted count costs under 1%
  (298,177 against 295,275 ins on 1,024 records); `BinCodec` staying stateless costs
  150–200 ns over eleven failed attempts on a 16 KiB blob.

Not checked by anyone: non-default features, 32-bit, MSRV 1.85 and `no_std` builds of any
prototype.

### Phase 0: instruments (2026-11-12)

`benches/probe.rs` is a new bench executable of fixed-work modes, and `benches/probe.sh`
runs every mode under `perf stat` and prints instructions and cycles per unit. It is
separate from the Criterion executables, which are untouched: their timings stay comparable
with the 0.10.0 baseline, and `BNB_DISPATCH_PROBE=buffered` in `bitstream_bench` still reads
270.22 / 1243.22 / 148.91. What it adds over that probe:

- struct decoders (`fixed44`, `packet`, `sub_byte`, `ipv4ish`, `dmr`, `records_1024`), not
  only one enum;
- the shared attempt compiled out of line, as consumers get it (`prefix_*`, `pull_short`,
  `stream_*`, `codec_decode`). That holds in the `tokio` build, which is therefore the
  gate's build; in a default-features build of the same file `pull_short` and the
  `prefix_*fixed44` modes inline it, and `pull_short` reads 177 instructions and 40 cycles
  instead of 239 and 110. The script looks for the call in each of those modes' code and
  names any mode that lost it;
- a fixed message through `BitBuf` at chunk 44, 5 and 1 (`buffered_fixed44_*`), the case a
  per-message availability check (row 4) has to hold. These inline the attempt in both
  builds, as row 9 says of the old probe, so the successful out-of-line attempt is covered
  only by `prefix_*`, `stream_*` and `codec_decode`;
- results consumed by reference. The dispatch benchmarks in `bitstream_bench` pass an
  80-byte `Result` to `black_box` by value, which adds a constant to both sides of every
  comparison; they are left as they are so they stay comparable with the tag;
- encode modes for phase 3.

**The gate, by tier** (the root `AGENTS.md` rule applied to this crate; it replaces reading
§13's "strict gate" as covering every benchmark alike). Each mode carries its tier:

- `happy`, valid input: a repeatable regression needs a fix or the owner's approval.
- `expected`, failures a working consumer meets constantly (`Incomplete` while streaming,
  dispatch fallthrough): gated exactly as `happy`.
- `terminal`, rejections (`*_invalid`, `magic_miss`, `fixed44_short`, `fixed44_trailing`):
  measured and recorded with every change, and bounded (no new allocation, no growth with
  input size), but a marginal regression that buys a `happy` or `expected` gain is accepted
  without escalation. Both numbers go in the record. Owner decision, 2026-11-12: dispatch
  misses stay `terminal`. The rule also calls "a probe that usually misses" a hot path, and
  a consumer that tries one type after another meets exactly these misses, but a miss
  already costs 118 to 188 cycles against 33 for a hit, most of it building the error:
  gating would protect such a consumer from a few percent, where a non-allocating "is this
  one of mine" check would save it most of each miss. That check is the rule's escape
  hatch, on the ROADMAP (0.10.x item 6) for when a consumer needs it. A decode that
  succeeds is never a miss, whichever variant it lands on: tag dispatch is a `match`,
  uniform-width magic one read and an `==` chain, and only mixed-width byte-string magics
  pay per earlier variant (`magic_eighth`, tier `expected`).
- `encode`: as `happy`.

A change is measured with this probe on parent and candidate (instructions decide; cycles
indicate, and move with placement), with the Criterion suites by §19's method, and with the
old buffered probe. Adding a mode can move the others, since the modes share monomorphized
readers as a consumer's message types do: re-baseline after adding one.

Baseline, `--features tokio`, Rust 1.99.1, Threadripper 7970X, core 61, process startup
subtracted. Instructions agree across two runs to 0.03 per unit; cycles are the lower of
two. The tag and `main` differ by the depth budget (one instruction per out-of-line attempt
and about one per counted record; nothing in the modes that inline the attempt) and the
frame cap (the rest of `codec_decode`'s four).

| Mode | Tier | 0.10.0 ins/unit | `839ccecf` ins/unit | `839ccecf` cycles/unit |
|---|---|---:|---:|---:|
| `fixed44` | happy | 506.99 | 507.99 | 97.6 |
| `packet` | happy | 359.99 | 360.99 | 74.9 |
| `sub_byte` | happy | 853.99 | 853.99 | 160.7 |
| `ipv4ish` | happy | 768.99 | 769.99 | 173.9 |
| `dmr` | happy | 707.99 | 708.99 | 154.0 |
| `records_1024` | happy | 273,617.44 | 274,671.40 | 49,060.1 |
| `numeric_valid` | happy | 142.00 | 142.00 | 33.0 |
| `bytes_valid` | happy | 141.00 | 141.00 | 34.0 |
| `magic_first` | happy | 207.99 | 207.99 | 40.0 |
| `prefix_envelope` | happy | 295.99 | 296.99 | 99.7 |
| `prefix_eof_envelope` | happy | 281.99 | 282.99 | 67.0 |
| `prefix_fixed44` | happy | 606.99 | 607.99 | 159.7 |
| `buffered_fixed44_whole` | happy | 739.06 | 739.06 | 182.1 |
| `magic_eighth` | expected | 712.99 | 712.99 | 128.0 |
| `buffered_fixed44_5` | expected | 7,558.98 | 7,558.98 | 1,987.2 |
| `buffered_fixed44_1` | expected | 35,376.96 | 35,376.97 | 9,290.0 |
| `pull_short` | expected | 237.99 | 238.99 | 110.2 |
| `prefix_short_envelope` | expected | 201.99 | 202.99 | 88.5 |
| `prefix_short_fixed44` | expected | 1,781.99 | 1,782.99 | 438.9 |
| `numeric_invalid` | terminal | 237.99 | 237.99 | 114.9 |
| `bytes_invalid` | terminal | 338.99 | 338.99 | 144.4 |
| `magic_miss` | terminal | 782.97 | 782.97 | 188.0 |
| `fixed44_short` | terminal | 1,675.98 | 1,676.97 | 355.0 |
| `fixed44_trailing` | terminal | 507.99 | 508.99 | 94.3 |
| `fixed44_encode` | encode | 758.99 | 758.99 | 164.1 |
| `packet_encode` | encode | 1,089.97 | 1,089.97 | 275.3 |
| `ipv4ish_encode` | encode | 1,556.97 | 1,556.98 | 353.6 |
| `dmr_encode` | encode | 2,733.95 | 2,733.95 | 592.1 |
| `stream_16x40_whole` | happy | 369.55 | 370.55 | 172.7 |
| `stream_16x40_64` | expected | 636.80 | 638.43 | 292.1 |
| `stream_16x40_1` | expected | 7,316.84 | 7,319.84 | 1,651.9 |
| `codec_decode` | happy | 466.99 | 470.99 | 134.7 |
| `codec_encode` | encode | 1,553.99 | 1,553.99 | 361.8 |

The raw output is kept on the measuring machine only, beside the 0.10.0 Criterion baseline
(the tag's numbers come from the tag's tree with this bench file added).

### Phase 1: changes nothing pays for (2026-11-12)

Three commits, each measured with the probe on its parent. The probe first gained three
modes, because review found the unaligned-read change reached cases nothing measured:
`unaligned_mid` and `unaligned_tail` (fields of 12 to 28 bits with sixteen bytes after them,
and at the very end of a buffer) and `unaligned_wide_tail` (a 108-bit field fourteen bytes
from the end). That makes 36 modes; decoding `Burst` on its own moved `dmr` from 708.99 to
730.99, and no other mode. The table below is the baseline from here on.

1. **`decode_attempt` builds its reader in place** (row 2). It checks the start once,
   reporting what `BitReader::seek_to_bit` reported, and each branch builds a reader that is
   already positioned. Two forms that still seek after building (one reader per branch; one
   reader inside the `IncrementalReader` both branches use) gave the same cycle gain but
   added three instructions per attempt where a consumer inlines the attempt
   (`buffered_fixed44_*`: +4, +29 and +133 per message); the form taken leaves those modes
   identical.
2. **`extract_unaligned_bits` loads its window whole** (row 3) when sixteen bytes lie behind
   the field's first byte, at any width. Within a buffer's last fifteen bytes it folds as
   before, in a `u64` when the field touches at most eight bytes: the guard alone cost a
   narrow tail read five instructions (454.99 → 459.99), and the `u64` fold turns that into
   a gain. The fold's 17th-byte arms became unreachable and are gone. Rejected: one path
   that zero-pads a short tail into a 16-byte array and loads that. It had the fewest
   instructions (`unaligned_tail` 378.99) and the most cycles (93 → 114): the load reads
   back bytes just stored one at a time, the same stall as row 2.
3. **`net::read_limit` and `shortfall` are `#[inline]`** (row 5).

Instructions per unit (`--features tokio`, core 61), parent, then after each commit, and
cycles before and after all three. The other 22 modes are instruction-identical
throughout, in every tier:

| Mode | Tier | Parent | 1 | 2 | 3 | Cycles |
|---|---|---:|---:|---:|---:|---|
| `dmr` | happy | 730.99 | 730.99 | 425.99 | 425.99 | 160.1 → 94.1 |
| `unaligned_mid` | happy | 509.99 | 509.99 | 360.99 | 360.99 | 103.0 → 83.7 |
| `unaligned_tail` | happy | 454.99 | 454.99 | 414.99 | 414.99 | 93.0 → 72.0 |
| `unaligned_wide_tail` | happy | 603.99 | 603.99 | 408.99 | 408.99 | 138.3 → 93.5 |
| `prefix_envelope` | happy | 296.99 | 289.99 | 289.99 | 289.99 | 99.7 → 68.6 |
| `prefix_eof_envelope` | happy | 282.99 | 281.99 | 281.99 | 281.99 | 66.6 → 66.1 |
| `prefix_fixed44` | happy | 607.99 | 601.99 | 601.99 | 601.99 | 157.8 → 127.7 |
| `pull_short` | expected | 238.99 | 231.99 | 231.99 | 231.99 | 110.2 → 74.2 |
| `prefix_short_envelope` | expected | 202.99 | 195.99 | 195.99 | 195.99 | 88.4 → 53.1 |
| `prefix_short_fixed44` | expected | 1,782.99 | 1,775.99 | 1,775.99 | 1,775.99 | 439.4 → 412.9 |
| `stream_16x40_whole` | happy | 370.55 | 363.55 | 363.55 | 356.30 | 173.4 → 119.6 |
| `stream_16x40_64` | expected | 638.42 | 627.05 | 627.05 | 592.30 | 291.4 → 211.1 |
| `stream_16x40_1` | expected | 7,319.83 | 7,298.83 | 7,298.83 | 6,615.83 | 1,644.2 → 1,341.3 |
| `codec_decode` | happy | 470.99 | 463.99 | 463.99 | 463.99 | 135.2 → 111.5 |

The old buffered probe reads 270.24 / 1243.24 / 148.92, as before.

**Criterion**, §19's method against the 0.10.0 tag (three alternating pinned rounds, lowest
median; ns):

| Benchmark | 0.10.0 | Phase 1 | Ratio |
|---|---:|---:|---:|
| `dmr_frame/decode` | 30.97 | 16.16 | 0.52 |
| `stream_envelope/64/64` | 88.50 | 60.87 | 0.69 |
| `stream_envelope/1024/64` | 228.32 | 169.94 | 0.74 |
| `stream_envelope/16x40/64` | 984.92 | 758.75 | 0.77 |
| `stream_envelope/1024/1028` | 81.11 | 62.43 | 0.77 |
| `stream_envelope/16x40/704` | 623.83 | 500.93 | 0.80 |
| `stream_envelope/64/68` | 52.37 | 43.21 | 0.83 |
| `stream_envelope/16x40/1` | 5,341.8 | 4,438.2 | 0.83 |
| `stream_envelope/16384/64` | 2,240.9 | 1,867.3 | 0.83 |
| `stream_envelope/64/1` | 477.69 | 404.43 | 0.85 |
| `stream_envelope/1024/1` | 5,960.0 | 5,272.4 | 0.88 |
| `stream_envelope/16384/1` | 93,204 | 83,222 | 0.89 |
| `stream_envelope/16384/16388` | 617.72 | 558.87 | 0.91 |

Of the other 48 benchmarks, 42 read 0.95–1.05. The six outside were rerun in builds made
with `-C llvm-args=-align-all-functions=6` (§19's placement check): `byte_aligned/encode`
1.052 → 0.994, `counted_u32_256/encode` 1.116 → 0.986, `counted_u32_256/decode_hand` 1.323
→ 1.002, `counted_u16_1024/decode` 0.885 → 1.005, `incremental_envelope/1024/1500` 0.943 (not
rerun; faster). `enum_dispatch/bytes_invalid` stays at 1.084 there, with `integer_invalid`
at 0.917: §20's accepted depth-budget pair (1.076 and 0.915), which these commits leave
instruction-identical (`bytes_invalid`, `numeric_invalid` above). `incremental_envelope`
does not move (0.94–1.04): its attempt is inlined in that executable, which is why the old
harness never saw row 2.

**Tests.** `an_attempt_starts_where_it_is_told_and_rejects_a_start_past_the_end` pins the
start check, both `eof` values and the boundary; it passes against the old body too. The
`extract_bits` differential also runs each read with bytes after the field, so a narrow
read takes the whole-window path. Nineteen hand mutants killed: three on the attempt (the
bound, the start position, the error's position), eight on the window load (the spill
bound and never spilling, both 17th-byte shifts, both final shifts, both `lead` shifts),
three on the `u128` fold (the mask and both shifts), five on the `u64` fold (the
eight-byte bound, the mask and its width, both shifts). 856 tests with all
features, 780 with default; clippy in both; MSRV 1.85 and the `no_std` build.

**Independent review.** No blocking finding. Applied: the dead fold arms removed; the
unmeasured narrow and tail reads measured (which found the five-instruction cost above);
this record; the boundary assertions in the attempt's test; a comment that called
`shortfall` per-read. It confirmed error parity for every input of the attempt, and the
window arithmetic by an independent model over 2,045,760 cases.

### Research before phases 2 to 6 (2026-11-12)

Nine prototypes for the hypotheses and the unbuilt fixes, each in a scratch copy of
`da7a8aa6`, measured with the probe against a parent built from the same probe file
(instructions; cycles as ranges over three or more pinned rounds, taken while two other
measurements shared the machine) and with Criterion where a benchmark covers the path.
Every prototype below passed the 856-test suite, clippy, the `no_std` check and
`cargo +1.85 check` in its copy. Nothing is merged; the prototypes are kept on the
measuring machine.

| Question | Result | Verdict |
|---|---|---|
| Inline fast path on `IncrementalReader::read_into` | `prefix_fixed44` 601.99 → 569.99, `buffered_fixed44_whole` 739.06 → 707.06, `buffered_fixed44_1` −80 per message; 33 of 38 modes identical, none worse | Go |
| The same on `read_bytes` | Where the decoder is out of line the success reload goes: a new `pull_whole8` mode 354.99 → 327.99 and 111–115 → 83 cycles, and Criterion whole-message cases 0.82–0.92. The `expected` tier pays: `prefix_short_envelope` +25 (53–57 → 68–69 cycles), `pull_short8` +16, `stream_16x40_1` +36 | Not as built; one shape is untried (test the shortage inline before the aligned test). Retry after phase 2 |
| Keep the `Result` whole in `decode_exact` (row 7) | The copy does not go: it widens from 48 bytes to the whole 80, because rustc 1.99.1 does not build the decoder's `Result` in the return slot. `numeric_valid` 33 → 41–45 cycles, `fixed44` +8% | No-go. Row 7 is closed as not fixable from this function |
| Per-field error decoration through a cold out-of-line helper | Decoders grow (`Packet` 1,010 → 1,349 bytes, `SubByte` 4,932 → 10,655): each block gains a copy in and out, and the inliner spends the freed budget elsewhere. `records_1024` +68% cycles, `prefix_short_envelope` +74–85% | No-go |
| `#[cold]` alone on `BitError::in_field` | Fifteen gated modes lose 1 to 458 instructions (`ipv4ish` 769.99 → 722.99, `fixed44` → 491.99); seven gain 1 to 3; `records_1024` +2.2% cycles | Retry after phase 2: the gains are row 1's inlining arriving by accident |
| A size hint for `to_bytes` on variable-length and bare-derive messages | One allocation at any size (was 2 to 12) when every field's width is visible to the macro. `packet_encode` 1,089.97 → 545.97, `ipv4ish_encode` 1,556.97 → 817.97, `dmr_encode` 2,733.94 → 1,996.94; Criterion `byte_runs_2x255/encode` 0.30, `ipv4ish_message/encode` 0.43, `byte_aligned/encode` 0.46; every non-encode mode identical. The returned `Vec` has exact capacity. Does not reach `#[bin]` enums, `to_bytes_with` or the generic `encode_to_vec` behind the transports | Go, in phase 3. A flat 64-byte start capacity was rejected (9 allocations left on 1,024 records, 64 bytes held for a 5-byte message) |
| `BitBuf` compaction | Reproduced on a bounded buffer kept full: 78 / 610 / 10,400 ns per 44-byte message at 4.4 KiB / 64 KiB / 1 MiB, against 33–37 drained first. An unbounded buffer whose capacity fits the backlog exactly has the same cost. Compacting an unbounded buffer only when the dead prefix is at least the live bytes makes it 36–39 ns at every size, with no gated mode worse, for one reallocation and twice the peak capacity under backlog. A bounded buffer has no fix short of a ring or twice the storage | Owner decision (it rewrites two documented promises and doubles peak memory under backlog). The bounded case gets a documentation sentence either way |
| Transport reads past the 4 KiB scratch | `take(hint).read_to_end` into the buffer's own `Vec` when the hint is at least the scratch: a 1 MiB envelope takes 8 socket reads instead of 258 (357–370 → 146 µs), 64 KiB 17 → 3 (24 → 12.5 µs); it reads less past the message than today (exactly the hint). In memory the gain is 13–15%, none for a reader without `read_buf`. Two `happy` modes read worse in cycles through an unrelated inlining flip (`stream_16x40_whole` 116–121 → 134–138, `codec_decode` 111–115 → 133–136) | Retry after phase 2, with the sync and async paths together; §12.1 changes |
| `LimitedSource` | Forwarding `aligned_run`, byte runs built on it, and `#[inline]` on its `read_bits` with a cold boundary error: a windowed 44-byte message 2,095.98 → 601.98, a windowed 64-byte run 6,603.96 → 329.96, `Incomplete` through the window 2,128.96 → 1,450.96; nothing worse in any tier. 94,110-case differential identical, three mutants detected. Without the `#[inline]`, two failing modes pay 9 and 22 instructions | Go |
| Byte-string magic fallthrough | The generator emits one lend of the longest magic and prefix compares in order, falling back to today's chain when the lend is unavailable: `magic_eighth` 712.99 → 187.98, `magic_first` 207.99 → 159.99, `magic_miss` 782.97 → 274.96, a one-byte-at-a-time pull 5,771 → 4,270; `mixed_magic/slice_eighth` 27.19 → 9.60 ns. Nothing worse; the eight-variant decoder grows 2,354 → 4,071 bytes. 128,130-case differential identical, three mutants detected. Four other shapes regressed something (a runtime-only lend lost the first-byte early-out: `magic_eighth` → 827.99) | Go |

**What the research changed about the order.** Five of the prototypes moved gated modes by
flipping an inlining decision they did not touch (whether `decode_with` is inlined into
the attempt, whether `extract_bits` is inlined into `match_magic`). The parent's own
`magic_*` numbers are bistable: adding a mode that instantiates a second reader moved
`magic_eighth` from 712.99 to 1,237.98 with no source change. Phase 2 pins that boundary,
so it goes first, and the three "retry" rows are measured again on top of it. When reading
any count for a change of this kind, first compare which of `decode_with`, `decode_attempt`
and `pull` is inlined into which, in parent and candidate.

### Phase 2: inline typed reads on the slice readers (2026-11-12)

Row 1, for `BitReader` and the incremental reader behind `decode_prefix`, `BitBuf` and the
transports. `BytesReader` and `LimitedSource` are the rest of the phase and follow in the
next section. Two commits, each measured with the probe on its parent.

1. **`Source::read` is overridden on both readers and forced inline.** A value that is a
   whole number of bytes, read from a byte-aligned cursor, is one bounds check, one load
   and at most a byte swap (`BitReader::take_whole_bytes`): aligned whole bytes are the
   value big-endian or little-endian by the effective byte order alone, whatever the bit
   order, because each bit order's natural layout is swapped exactly when the declared
   order is the other one. A short read goes straight to the cold constructor `read_bits`
   uses (`read_error`, or `incomplete` on the incremental reader), so the error, its
   position and the cursor are unchanged. Every other read (a width that is not whole
   bytes, or an unaligned cursor) inlines the old body, `read_by_bits`, which is also what
   the trait's default calls for every other source. The bound is `bytes_in(len)`,
   `bits_in(len) / 8` written as a clamp, so the 32-bit saturation of §18 holds on this
   path too. `IncrementalReader::read_bits` is pinned inline with it, and the public
   inherent `BitReader::read` forwards to the override.
2. **`IncrementalReader::read_bytes` is inline down to the copy**, with the shortage tested
   first and built out of line (the shape the research left untried). Commit 1 makes
   decoders larger, and `Envelope`'s stops being inlined into `decode_attempt`. Out of
   line, it receives the `Vec` from `read_bytes` in three narrow stores and reloads it in
   one wide load: `perf annotate` put 93% of that decoder's samples on the instruction
   after the reload. Commit 2 removes the stall. On the phase 1 base alone it is not a
   gain (`prefix_envelope`, `stream_16x40_whole` and `codec_decode` +3 instructions,
   `stream_16x40_1` +18), which is why it follows commit 1 and is not phase 6's.

Instructions per unit (`--features tokio`, core 61): the phase 1 base, after each commit,
and cycles at the three points. The five encode modes are instruction-identical.

| Mode | Tier | Base | 1 | 2 | Cycles |
|---|---|---:|---:|---:|---|
| `fixed44` | happy | 507.99 | 225.99 | 225.99 | 100.4 → 47.0 → 47.1 |
| `packet` | happy | 360.99 | 184.99 | 184.99 | 75.3 → 42.3 → 43.1 |
| `sub_byte` | happy | 853.99 | 691.99 | 691.99 | 160.1 → 110.8 → 110.7 |
| `ipv4ish` | happy | 769.99 | 435.99 | 435.99 | 173.9 → 99.6 → 97.4 |
| `dmr` | happy | 425.99 | 421.99 | 421.99 | 91.1 → 94.4 → 92.1 |
| `unaligned_mid` | happy | 360.99 | 361.99 | 361.99 | 84.2 → 82.3 → 84.5 |
| `unaligned_tail` | happy | 414.99 | 415.99 | 415.99 | 72.1 → 72.1 → 71.0 |
| `unaligned_wide_tail` | happy | 408.99 | 405.99 | 405.99 | 93.6 → 92.5 → 92.3 |
| `records_1024` | happy | 274,670.74 | 108,737.71 | 108,737.81 | 49,160.7 → 17,226.8 → 17,254.0 |
| `numeric_valid` | happy | 142.00 | 100.00 | 100.00 | 33.2 → 31.0 → 31.1 |
| `bytes_valid` | happy | 141.00 | 102.00 | 102.00 | 34.1 → 30.1 → 30.1 |
| `magic_first` | happy | 207.99 | 169.99 | 169.99 | 40.0 → 36.3 → 36.3 |
| `prefix_envelope` | happy | 289.99 | 270.99 | 252.99 | 68.3 → 98.2 → 56.9 |
| `prefix_eof_envelope` | happy | 281.99 | 242.99 | 242.99 | 67.0 → 56.0 → 55.8 |
| `prefix_fixed44` | happy | 601.99 | 286.99 | 286.99 | 129.1 → 65.0 → 63.3 |
| `buffered_fixed44_whole` | happy | 739.06 | 424.06 | 424.06 | 183.1 → 109.3 → 107.5 |
| `stream_16x40_whole` | happy | 356.30 | 337.30 | 319.30 | 121.9 → 133.6 → 117.1 |
| `codec_decode` | happy | 463.99 | 444.99 | 426.99 | 116.2 → 138.2 → 106.6 |
| `magic_eighth` | expected | 712.99 | 674.99 | 674.99 | 120.9 → 119.1 → 118.0 |
| `buffered_fixed44_5` | expected | 7,558.97 | 3,886.04 | 3,886.04 | 1,977.8 → 1,176.0 → 1,155.2 |
| `buffered_fixed44_1` | expected | 35,376.96 | 18,201.95 | 18,201.95 | 9,220.9 → 5,596.4 → 5,240.7 |
| `pull_short` | expected | 231.99 | 169.99 | 153.99 | 74.5 → 48.3 → 45.0 |
| `prefix_short_envelope` | expected | 195.99 | 167.99 | 151.99 | 53.1 → 66.0 → 61.5 |
| `prefix_short_fixed44` | expected | 1,775.99 | 615.99 | 615.99 | 425.0 → 166.6 → 167.7 |
| `stream_16x40_64` | expected | 592.30 | 555.80 | 527.80 | 206.3 → 223.9 → 210.4 |
| `stream_16x40_1` | expected | 6,615.83 | 6,557.83 | 6,526.83 | 1,356.4 → 1,379.3 → 1,355.8 |
| `numeric_invalid` | terminal | 237.99 | 238.99 | 238.99 | 116.2 → 117.3 → 114.8 |
| `bytes_invalid` | terminal | 338.99 | 340.99 | 340.99 | 145.0 → 146.5 → 145.9 |
| `magic_miss` | terminal | 782.97 | 786.97 | 786.97 | 187.4 → 189.0 → 186.1 |
| `fixed44_short` | terminal | 1,676.97 | 552.97 | 552.97 | 369.6 → 138.1 → 138.3 |
| `fixed44_trailing` | terminal | 508.99 | 226.99 | 226.99 | 94.5 → 42.2 → 39.7 |

The old buffered probe reads 246.22 / 1227.22 / 124.91 after both commits, against
270.22 / 1243.22 / 148.91: lower at every chunk size, so the owner decision the plan
reserved for it was not needed.

**What pays.**

- `unaligned_mid` and `unaligned_tail` (`happy`) gain one instruction per decode, with
  cycles level or lower. Their fields are 4 to 28 bits wide and take the inlined
  `read_by_bits`; the bare inline pin below costs the same one.
- `prefix_short_envelope` (`expected`): 44 fewer instructions and 53 → 61–62 cycles. The
  failing attempt's 80-byte error is now copied by the out-of-line decoder and again by
  `decode_attempt`, a stalled reload each; with the decoder inlined into the attempt, as
  the base happened to have it, it was copied once. Nothing in this crate can force that
  inlining (generated code carries no hints, §19). The realistic waits improve or hold:
  `pull_short`, `buffered_fixed44_1` and `_5`, `prefix_short_fixed44`, `stream_16x40_1`.
  The fix for the copy itself is a smaller error, which is 0.11's.
- Three `terminal` modes gain 1 to 4 instructions.
- Code size. `Fixed44`'s incremental decoder goes from 270 to 849 instructions, because
  each whole-byte field carries the aligned load and the inlined unaligned fallback. The
  probe's `.text` grows 335,827 → 347,923 bytes and `bitstream_bench`'s 2,311,382 →
  2,315,158.
- `pull_short` now has the attempt inlined into `pull` (`probe.sh` reports it), so it no
  longer measures the out-of-line attempt; `prefix_short_envelope` still does.

**Variants built and rejected**, each a probe build against the same parent:

| Variant | Result |
|---|---|
| `#[inline(always)]` on the trait's default `read` and on `IncrementalReader::read_bits`, nothing else | `fixed44` 371.99, `records_1024` −29%, but the old buffered probe 287.22 / 1344.22 / 165.91, and `prefix_envelope`, `stream_16x40_whole` and `codec_decode` +22–58% in cycles at unchanged instructions (the stall of commit 2). `Fixed44`'s decoder 828 instructions: both bit orders' loads and a runtime byte swap at every field |
| The whole-byte load through `aligned_run`, anything it declines to the general path at the optimizer's discretion | `sub_byte` unchanged at 853.99 (its fields were not inlined); buffered probe 251.22 / 1264.22 / 129.91, the middle figure above the base |
| The same, with the declined read forced out of line | `dmr` +24 (+5.6%), `unaligned_wide_tail` +25, buffered probe at one-byte chunks 1428.22: a short read ran the fast path's check, then a call, then the general check |
| As taken, but an unaligned whole-byte read out of line | The smallest code (`Fixed44`'s decoder 462 instructions, `.text` 340,163) and the lowest counts on aligned shapes (`fixed44` 215.99, `numeric_valid` 92.00; buffered probe 238.22 / 1187.22 / 116.91, and 283–287 cycles per message at one-byte chunks against 349–357). `dmr` 425.99 → 450.99 and 91 → 104 cycles, `unaligned_wide_tail` +26. Not taken, because a gated `happy` mode pays; why one out-of-line 48-bit read costs that much was not found, and the variant is worth another look if code size becomes the constraint |
| As taken, bounded by `bits_in(len) / 8` instead of the clamp | The old buffered probe at one-byte chunks 1244.22, one above the base; `records_1024` +2.8% |

**Criterion**, §19's method against the 0.10.0 tag (three alternating pinned rounds, lowest
median; ns). These are cumulative with phase 1. The machine was under heavy unrelated load
for part of the run (15-minute load average over 190 on 64 threads), so ratios near 1 are
weaker than usual; the gains are far outside that.

| Benchmark | 0.10.0 | Phase 2 | Ratio |
|---|---:|---:|---:|
| `fixed_44/decode` | 19.91 | 9.12 | 0.46 |
| `dmr_frame/decode` | 31.51 | 16.08 | 0.51 |
| `stream_envelope/16x40/704` | 664.27 | 376.31 | 0.57 |
| `ipv4ish_message/decode` | 35.27 | 21.18 | 0.60 |
| `stream_envelope/64/68` | 56.83 | 35.33 | 0.62 |
| `byte_aligned/decode` | 15.08 | 9.50 | 0.63 |
| `stream_envelope/16x40/64` | 1,007.8 | 647.26 | 0.64 |
| `stream_envelope/64/64` | 89.26 | 57.79 | 0.65 |
| `sub_byte/msb_decode` | 34.11 | 22.87 | 0.67 |
| `sub_byte/lsb_decode` | 31.63 | 21.59 | 0.68 |
| `stream_envelope/1024/1028` | 84.29 | 57.56 | 0.68 |
| `incremental_envelope/64/68` | 30.00 | 24.24 | 0.81 |
| `stream_envelope/16x40/1` | 5,451.6 | 4,353.3 | 0.80 |
| `incremental_element_replay/64/1` | 86,681 | 72,840 | 0.84 |
| `incremental_envelope/1024/1` | 12,248 | 11,456 | 0.94 |
| `enum_dispatch/integer_valid` | 9.02 | 8.33 | 0.92 |

The other `stream_envelope`, `incremental_envelope` and `incremental_element_replay` cases
read 0.79–0.95, except three large-payload ones at 0.98–1.01. Every encode benchmark reads
0.96–1.03.

Four benchmarks read above 1.05 in that run or in the function-aligned rerun (§19's
placement check) and were measured again on the idle machine, three alternating rounds:

- `counted_u32_256/decode_hand` 1.355, aligned 0.965. It does not call this crate; phase 1
  saw the same figure. Placement.
- `byte_runs_2x255/decode` 1.041, aligned 1.061; idle 0.99 and 0.98. Noise from the load.
- `enum_dispatch/bytes_invalid` 1.105, aligned 1.050; idle, against the phase 1 base, 1.01
  (37.47 → 37.84). It is §20's accepted figure against the tag and does not move here.
- `enum_dispatch/buffered_chunk_1` 1.074, aligned 1.050; idle, against the phase 1 base,
  1.03 in each of three rounds (1,080 → 1,113 ns lowest). This one is real and is in the
  `expected` tier: 16 fewer instructions per message (1243.22 → 1227.22) and about 3% more
  time, cycles 339–371 → 349–357 on the fixed-work probe. Four of its five attempts fail,
  and each failing attempt's error takes the extra copy described under
  `prefix_short_envelope`. `buffered_chunk_5` reads 0.98 and `buffered_chunk_80` 0.96–0.98
  in the same builds. The variant that kept unaligned reads out of line read 283–287
  cycles here, by keeping the enum's decoder small enough to inline into the attempt.

**Tests.** `read_takes_whole_bytes_exactly_as_the_bit_path_does` reads sixteen types
(1 to 128 bits, a 24-bit type declaring each byte order, zero-width and 136-bit) at every
cursor of every buffer length from 0 to 20 bytes, in all four layouts, through the trait
on both readers and through the inherent method, against `read_by_bits`: value or error
kind and position, and the cursor afterwards.
`the_incremental_reader_reads_a_byte_run_as_the_per_byte_loop_does_or_waits` does the same
for `read_bytes`, including `usize::MAX` bytes; `bytes_in_is_bits_in_counted_in_bytes` pins
the clamp at the saturation edge, which no host buffer can reach. Twenty-one hand mutants
killed: the byte-order arms swapped, the big-endian value left-aligned, the type's own
order ignored or overridden, the cursor not advanced or advanced by bytes, the fast path
taken unaligned, too wide or for a sub-byte width, both shortage errors, the clamp removed
or one early, the run one byte late or unbounded, four on `read_bytes`, and the byte order
dropped from the general path. The mutant that stops the cursor advancing makes a counted
read allocate without bound: the runner caps the test binary's address space and time.
859 tests with all features, 783 with default; clippy in both; MSRV 1.85 and the `no_std`
build for `thumbv7em-none-eabi`, a 32-bit target.

**Independent review.** No blocking finding; the equivalence claim was not refuted. The
reviewer derived the byte-order identity from `extract_bits` and `apply_byte_order` for
all four layouts, checked that every writer of `bit_pos` keeps it within `bits_in(len)`,
and ran an independent model over 7,435,800 cases (widths 0 to 1024, lengths 0 to 24,
every cursor, both readers, `read_bytes` to `usize::MAX`) with overflow checks on. Applied:
the two changes split into two commits; `extract_bits`'s doc and §10 no longer call it the
only path; this record; a comment that claimed the stall on every blob; zero-width and
72- and 104-bit types in the test; a `debug_assert!` on `take_whole_bytes`'s precondition.
Not done: nothing pins that the inherent `read` takes the fast path (its mutant is
behaviour-equivalent).

Not measured at these two commits: `BytesReader`, `LimitedSource` and the stream readers,
which kept the default `read` and whose benchmarks moved only within the noise above. The
next section takes the first two.

### Phase 2, continued: windows and `BytesReader` (2026-11-12)

The two readers the phase had left. The probe first gained eleven modes (47 with `tokio`):
`Fixed44` and `Envelope` behind `Source::limit_bytes`, whole, as a prefix, one byte short,
and in a region too short or too long; and `Fixed44`, `SubByte` and `Envelope` from a
`BytesReader`. Adding them moved the three magic modes in that executable with no library
change (`magic_first` 169.99 → 281.99, `magic_eighth` 674.99 → 1,199.98, `magic_miss`
786.97 → 1,258.96): `match_magic` stopped inlining `extract_bits`. They read their earlier
figures again in every build below. That boundary is the one the magic fallthrough of
phase 6 replaces, and it is still unpinned; a one-line pin (the magic's bytes through the
typed read) was tried and gave 731.99 instructions but 195 cycles on `magic_eighth`
against 118 in the build that inlines, so it was not taken.

1. **`LimitedSource` forwards the typed read and lends runs.** `read` is the window check,
   then the wrapped reader's own `read`, forced inline as that one is; left at `#[inline]`
   it stayed a call per field (`limit_fixed44` 432.98 against 308.98). `aligned_run` lends
   the wrapped reader's run when it lies inside the window, and `read_bytes` and
   `read_into` copy such a run whole. Anything else runs the per-byte loop through the
   window, so errors, positions and the retry hint are that loop's. The wrapped reader's
   own `read_bytes` is not used: over the incremental reader it reports the whole
   shortfall, where the loop reports the first missing byte. The boundary error is built
   out of line. This is the research row's change plus the `read` override.
2. **`BytesReader` overrides `read`**, forced inline: the slice reader built over the bytes
   in hand, the cursor checked as before (it may be past the end), then that reader's
   typed read. `read_bits` is `#[inline]`.

Instructions per decode, the 47-mode base, then after each commit. The other 36 modes are
instruction-identical at both, the magic modes at their earlier figures.

| Mode | Tier | Base | 1 | 2 | Cycles |
|---|---|---:|---:|---:|---|
| `limit_fixed44` | happy | 2,088.98 | 308.98 | 308.98 | 392.4 → 74.1 |
| `limit_prefix_fixed44` | happy | 2,133.98 | 341.98 | 341.98 | 412.4 → 92.2 |
| `limit_envelope` | happy | 6,598.96 | 284.96 | 284.96 | 1,219.7 → 93.9 |
| `limit_prefix_envelope` | happy | 6,625.96 | 311.96 | 311.96 | 1,195.0 → 71.0 |
| `limit_prefix_short_fixed44` | expected | 2,119.96 | 836.96 | 836.96 | 456.1 → 191.3 |
| `limit_prefix_short_envelope` | expected | 6,605.96 | 3,114.96 | 3,114.96 | 1,209.7 → 658.2 |
| `limit_fixed44_short` | terminal | 2,058.96 | 786.96 | 786.96 | 458.4 → 211.1 |
| `limit_fixed44_long` | terminal | 2,095.96 | 314.96 | 314.96 | 397.1 → 71.8 |
| `bytes_reader_fixed44` | happy | 849.98 | 849.98 | 271.98 | 178.6 → 55.1 |
| `bytes_reader_sub_byte` | happy | 1,867.98 | 1,867.98 | 772.98 | 365.8 → 123.1 |
| `bytes_reader_envelope` | happy | 369.98 | 369.98 | 268.98 | 103.2 → 97.3 |

For scale, the same messages from a slice read 225.99 (`fixed44`), 691.99 (`sub_byte`) and
252.99 (`prefix_envelope`). The probe's `.text` grows 377,987 → 388,147 bytes. A wait
through a window is still 3,115 instructions for a 64-byte run: the run is not all there,
so the per-byte loop runs until the first missing byte, which is what keeps its hint.

Criterion was not run: no benchmark or crate outside this one uses either reader, and the
36 earlier modes do not move.

**Tests.** `a_window_reads_as_its_per_bit_form_does_over_both_slice_readers` runs eleven
typed reads (4 to 128 bits, own byte orders, zero-width, 136-bit), `read_bytes` to
`usize::MAX`, `read_into` and `aligned_run` at every cursor of windows of 0 to 18 bytes
over buffers of 1 to 20, in all four layouts, over both slice readers, against
`PlainWindow`, the window as it was (only `read_bits` its own).
`a_window_reports_its_boundary_at_the_cursor_and_lends_up_to_it` pins what that model
shares with the window: the boundary error's fields and position, and that an exact-fit
run is lent. `component::bytes_adapters::a_typed_read_is_the_read_taken_bit_by_bit_at_any_cursor`
does the same for `BytesReader`, to nine bits past its end. Fourteen hand mutants killed
(the window check off by one each way, absent, capped at 128 and in bytes; the boundary
error's fields and position; a run lent across the window's end or declined on an exact
fit; both fallbacks sent to the wrapped reader; `BytesReader` without its seek check,
without advancing, without its layout). 862 tests with all features, 785 with default;
clippy in both; MSRV 1.85 and the 32-bit `no_std` build.

**Independent review.** No blocking finding. The reviewer built one harness against the
parent and against the change and compared transcripts: identical over about 0.9 billion
records, with windows over every reader in the crate (and a window in a window), four
layouts, windows to `usize::MAX / 8` bytes, `decode_vec` of seven element types and
overflowing counts. It confirmed the window's boundary error wins over the input's before
and after, and that using the wrapped reader's `read_bytes` would differ only over the
incremental reader. Applied: an 18-byte window and both byte orders in the test (a
mutant capping the checked width at 128 had survived); this record and the roadmap
corrections; the two changes as two commits; `Source::read_bytes`'s list of bulk copiers;
the test seeking through the window; `past_end` in the type's own `impl`; the
`BytesReader` test moved to `component`.

### Phase 6: byte-string magic fallthrough (2026-11-12)

Taken first in phase 6 because its inlining was the last unstable boundary (the probe's
magic modes moved by 60–78% twice with no library change, above). The change is the
research row's as built there: the generator's single look at the longest magic, and the
chain's own magics compared where they lie.

- **`peek_magic`** (`__private`, hidden). When the source lends the longest magic's worth
  of bytes (`aligned_run`: a byte-aligned cursor, all of them in memory), the winner is
  the first magic in declaration order that the lent bytes start with, or none; the
  cursor is left after the winner's magic, or at the start. No read of the chain can fail
  on such input, so this is the chain's answer. Otherwise it consumes nothing and says so.
- **`match_magic`** is forced inline and takes a lent run of the magic's own length when
  there is one: compare, and seek back on a miss. Anything else is the old loop, now
  `match_magic_per_byte`, out of line. This is what the chain runs when the longest magic
  is not there (a message shorter than it, the end of input, a buffer still filling).
- **The generator** (mixed-width magics or a fallback variant, the path already bounded
  on `SeekSource`) emits the winner's index from `peek_magic` or else from the chain, then
  one `match` on it; the fallback or the no-match error is the `_` arm. No inline hint in
  generated code.

The probe first gained four modes (51 with `tokio`): the eight `MixedMagic` variants
pulled from a bounded `BitBuf`, fed whole and one byte at a time, and an enum whose whole
message is shorter than its longest magic. Adding them moved no existing mode.
Instructions per unit, base and candidate; the other 44 modes are instruction-identical.

| Mode | Tier | Base | Candidate | Cycles |
|---|---|---:|---:|---|
| `magic_first` | happy | 169.99 | 119.99 | 37.0 → 31.1 |
| `magic_short_first` | happy | 166.99 | 105.99 | 29.0 → 18.6 |
| `buffered_magic_whole` | happy | 553.98 | 185.36 | 107.6 → 51.1 |
| `magic_eighth` | expected | 674.98 | 149.98 | 117.0 → 34.0 |
| `magic_short_third` | expected | 310.99 | 228.99 | 53.0 → 39.0 |
| `buffered_magic_1` | expected | 4,969.05 | 3,569.67 | 1,228.8 → 997.3 |
| `magic_miss` | terminal | 786.96 | 276.96 | 190.9 → 138.7 |

Criterion, three alternating pinned rounds against the parent, lowest median:
`mixed_magic/slice_eighth` 25.42 → 8.83 ns (0.35), `mixed_magic/slice_first` 9.50 → 8.62
(0.91); `enum_dispatch/bytes_valid`, `bytes_invalid` and `mixed_1_in_16_invalid`, which
are uniform-width and do not take this path, 1.00, 0.97 and 1.00.

What pays: code size. The eight-variant decoder grows from 969 to 1,377 instructions (the
constant compares of the look, plus the chain kept for when the look is unavailable); the
probe's `.text` 400,051 → 404,499 bytes. Nothing pays in instructions or cycles in any
tier. A variant that is the eighth of eight now costs about what the first does, so
declaration order no longer matters to speed when the longest magic is in memory.

**Tests.** `unit::magic_dispatch_from_a_lent_run_picks_what_the_byte_by_byte_chain_picks`
runs the generated control flow by hand (look, else chain) against the per-byte chain
over both slice readers, from a byte boundary and from inside a byte, on three magic sets
(a magic that is a prefix of a later one, of an earlier one, and the empty magic) with
every magic whole, cut at each length and with each byte changed.
`unit::a_peek_that_cannot_see_the_longest_magic_consumes_nothing` pins the look alone.
`macro_::mixed_width_magics_pick_the_same_variant_through_every_reader` decodes a generated
enum (four magics of two and six bytes, one shadowed, and a typed fallback) on 7,457
inputs through the slice reader in both bit orders, a window that ends inside a magic the
buffer could still lend, `BitBuf`, `BytesReader` and `decode_prefix`, against the variant
and length worked out by hand. Eleven hand mutants killed, eight in the runtime (the
compare inverted, a miss left consumed, a suffix match, the last match, the wrong index
for none, the cursor left at the start or after the longest magic, the shortest magic
looked at) and three in the generator (the chain reversed, the no-match index, the look's
answer ignored). 876 tests with all features across both crates, 788 with default; clippy
in both; MSRV 1.85 and the 32-bit `no_std` build.

**Independent review.** No blocking finding. One harness built against the parent and
against the change gave 593,112 identical lines (value, the error in full, cursor) over
nine enum shapes, every reader in the crate, `BitBuf` at every split, windows of every
length including one inside another and inside `decode_prefix`; a mutant build differed
on 46,331. It confirmed that every source that lends is seekable and byte-aligned when it
lends, so the seek after a lend cannot fail. Applied: `match_magic`'s doc no longer says a
mismatch stops at its first differing byte (true only of the per-byte loop); `peek_magic`'s
error section; this record; the integration test above, because the unit test copies the
generator's control flow and so cannot see a generator bug; two stale comments. Noted and
not fixed here, as it predates this change: a `ctx` enum on this path does not compile
(the generated `decode_with` bounds `SeekSource` and is called from a `Source`), so the
hybrid embedding of this block is unreachable today. ROADMAP §E already has it.

### Phase 3: a size hint for variable-length `to_bytes` (2026-11-12)

The first item of the encode phase, as researched. `to_bytes` of a fixed-length message
already allocates once (§19); a variable-length `#[bin]` struct or a lone
`#[derive(BitEncode)]` grew its buffer by doubling: 8, 16, 32, 64 bytes for a 44-byte
message.

- **The generator** sums an estimate, in bits, from the fields it can see, and `to_bytes`
  reserves it before writing. A `Vec` contributes its length times its element's width, a
  byte array its length, any other field its type's width, the struct's magic its own.
  A field contributes nothing when it writes nothing or a size the macro cannot see:
  ignored, `br(calc)`, a `temp` `Vec`, or anything `br_indeterminate` (context, `if`,
  `map`, `parse_with`, padding, alignment, seeks). Sums saturate.
- **A type's width** comes from `__private::BitLenHint::<T>`, whose inherent
  `__bnb_bits` reads `T`'s `FixedBitLen::BIT_LEN` when `T` has one, and falls back to a
  trait method returning zero when it does not. The hint therefore asks nothing of a
  field type that encoding did not already ask, and an element with no fixed width (a
  nested variable-length message) counts as nothing.
- **`encode_hinted_to_vec`** reserves the estimate rounded up to whole bytes, never less
  than eight (what a growing `Vec<u8>` starts with), nothing for a zero estimate, and
  skips a reservation the allocator refuses.

The probe gained `envelope_encode` and `records_encode` (53 modes with `tokio`); adding
them moved one existing mode by two instructions (`prefix_eof_envelope` 242.99 → 240.99).
Instructions per encode, base and candidate; every other mode is instruction-identical.

| Mode | Base | Candidate | Cycles |
|---|---:|---:|---|
| `packet_encode` | 1,089.96 | 547.96 | 256.4 → 127.6 |
| `ipv4ish_encode` | 1,556.96 | 819.96 | 343.4 → 183.4 |
| `dmr_encode` | 2,733.92 | 1,998.92 | 589.3 → 427.9 |
| `envelope_encode` | 767.98 | 351.98 | 177.4 → 86.3 |
| `records_encode` | 276,057.20 | 273,932.85 | 65,926.5 → 65,610.0 |

Criterion, three alternating pinned rounds against the parent, lowest median (ns). The
machine carried unrelated load during the second of two runs; both are given.

| Benchmark | Parent | Candidate | Ratio, both runs |
|---|---:|---:|---|
| `byte_runs_2x255/encode` | 84.30 | 23.06 | 0.27, 0.30 |
| `ipv4ish_message/encode` | 91.44 | 38.44 | 0.42, 0.40 |
| `counted_u32_256/encode` | 84.78 | 57.76 | 0.68, 0.63 |
| `counted_u16_1024/encode` | 98.69 | 82.86 | 0.84, 0.81 |
| `fixed_44/encode` (not on this path) | 36.02 | 35.66 | 0.99, 1.03 |

The two hand-written `encode_hand` baselines read 0.98–1.02. The probe's `.text` grows
406,291 → 406,947 bytes.

**What it does not do, and what pays.**

- The estimate is exact only when every field's width is visible. When part of the
  message is invisible to it (a `Vec` of variable-length messages, an `if` field) the
  buffer still grows, from the estimate instead of from eight bytes. As first built it
  reserved the bare estimate, and a small partial one then cost one allocation *more*
  than before (five became six for a 103-byte message with a two-byte estimate, measured
  by the reviewer with a counting allocator). The floor of eight removes that: growth from
  an estimate of at least eight bytes never takes more steps than growth from eight.
- The estimate never exceeds what a successful encode writes, but it is reserved before
  encoding can fail. A `Vec` of zero-sized elements with a non-zero wire width (a unit
  struct with a magic) and a length its count prefix cannot hold now asks for the size
  that encode would have written before it reports the prefix error; a request the
  allocator cannot meet is skipped.
- Not hinted: `to_canonical_bytes` of a variable-length message, `to_bytes_with`,
  `#[bin]` enums, and the generic `encode_to_vec` behind the socket transports
  (`BinCodec` no longer goes through it; next section).

**Review findings, all fixed before the commit.** The independent review compared old and
new on 22 message shapes (bytes and errors identical) and found three blocking defects
in the prototype as researched:

1. The width probe was a method named `bits`, found by name in the user's scope. A
   blanket `trait BitSize: Sized { fn bits(self) -> usize }` beside a message hijacked the
   hint (a five-byte message requested 4 GiB), and a `&self` one made a `Vec` of
   variable-length elements ambiguous (E0034) where the parent compiled. The method is
   `__bnb_bits`, and `tests/bin_hygiene.rs` holds both traits in scope.
2. The generated block carried `#[allow(unused_imports)]` on the fallback trait's import,
   which is a hard error (E0453) in a crate that `forbid`s the lint. The attribute is
   gone (an unused import in a proc-macro's output is not reported), and
   `tests/bin_presize.rs` forbids the lint over a message that never uses the trait.
3. Two branches of the field filter were unpinned (`br(calc)` and the indeterminate
   kinds): mutants that counted those fields survived. Both now have a test.

Also from review: a byte array's term is `usize::saturating_mul(N, 8)` (the first form
typed an unsuffixed length as `i32` and overflowed at compile time for `[u8; 1 << 61]`,
which the parent accepted), and the floor of eight above.

**Tests.** `tests/bin_presize.rs`: exact capacity for a message with a magic, two counted
runs (bytes and fixed-width records), a byte array and an ignored field at four sizes; for
a lone derive ending in a partial byte; for messages with a `br(calc)` or a peeked field;
and a message with an unsizable field, which only grows. Three of these fail on the
parent. `unit::a_hinted_encode_reserves_its_bytes_once_and_shrugs_off_a_hint_it_cannot_meet`
pins the rounding, the floor, the zero case and an unmeetable hint. Fourteen hand mutants
killed, six in the runtime (rounding down, no reservation, bits taken as bytes, a fatal
reservation, no floor, a floor for zero) and eight in the generator (a `Vec` counted by
length only or not at all, a byte array in bytes, a plain field or the magic dropped, an
ignored, `br(calc)` or indeterminate field counted). 882 tests with all features across
both crates, 794 with default; clippy in both; MSRV 1.85 and the 32-bit `no_std` build.
The reviewer also built a `#![no_std]` consumer under `deny(warnings)` and about thirty
allow-by-default lints, and clippy pedantic, nursery and restriction on generated code:
identical diagnostics old and new.

### Phase 3: `BinCodec` encodes in place (2026-11-12)

Row 6 for the `tokio_util` codec. `BinCodec::encode` built each message in a `Vec` that
grew by doubling (the generic `encode_to_vec` has no size to reserve) and then copied it
into `dst`. It now writes into `dst` directly.

- **`FrameWriter`** (private, with the `tokio` feature) is a `Sink` over the tail of a
  `BytesMut`. Its positions count from the message's first bit, as a `BitWriter`'s do, so
  alignment, padding and error positions do not depend on what the buffer already holds.
  It appends aligned byte runs whole and lends a tail for bulk integer runs, as
  `BitWriter` does.
- **The write primitive is shared.** `emit_bits` and `emit_unaligned_bits` are generic over
  a three-method private trait (`len`, `push`, `or_into`) implemented for `Vec<u8>` and
  for that tail. `BitWriter::write_bits` keeps its own body: routed through a helper
  shared with the frame writer, `dmr_encode` gained one instruction, and with its own
  body every mode but the codec's is instruction-identical.
- **A message stays only if the encode finished.** The writer takes back what it appended
  unless `keep` is called, in its `Drop`, so an encode error and a panic unwinding out of
  an encoder both leave `dst` holding whole messages only. The first version truncated
  in `encode`'s error branch; review showed a panic inside `bit_encode` (a `write_with`
  closure) then left the partial message behind, and a `Framed` that outlives the unwind
  sent it ahead of the next message. The old path never touched `dst` before success.

`codec_encode` (a 44-byte fixed message into a `BytesMut` with room) reads 1,553.98 →
1,069.98 instructions and 353 → 181 cycles; the other 52 modes are instruction-identical.
There is no Criterion benchmark of the codec's encoder, and none was added: the reviewer
counted allocations per encode, old against new, for 44 bytes, 64 KiB and 4 MiB over five
buffer states, and the new path never allocates more (0 to 20 against 4 to 21).

What is left on the table: the same message through `to_bytes` is 758.98 instructions.
The difference is one `extend_from_slice` per pushed byte, where a `Vec` push is an
inlined store; phase 3's inline `Sink::write` (a whole-byte value appended in one call)
is where that goes, for both writers.

**What pays.**

- Capacity. `dst` used to receive each message in one `extend_from_slice`, which reserves
  exactly; it now grows as the message is written, by doubling. A 64 KiB message into an
  8 KiB buffer leaves a capacity of 131,072 where it left 65,537 (measured by the
  reviewer): up to twice the message, once, on a buffer `Framed` reuses.
- A generated encoder is compiled once more per message type that goes through the codec
  (for the new sink). The probe's `.text` grows 406,947 → 409,267 bytes.
- `dst` holds part of a message during the call. Nothing in `tokio_util`'s `Framed`,
  `FramedWrite` or `UdpFramed` looks at it before `encode` returns (read by the reviewer).

**Tests.** `bytes_io::component::a_frame_written_in_place_is_the_frame_a_bit_writer_builds`
runs 400 rounds of twelve noisy writes (bits of every width, whole-byte widths, byte runs,
lent tails, too-wide widths) through both writers in all four layouts, behind zero to
three bytes the buffer already held: same results, cursor, bytes, and the earlier bytes
untouched. Two more pin the position bound and that an unkept writer takes its bytes
back. `framing::component`: a message appended twice after earlier content equals
`to_bytes`; a failing message leaves the buffer as it was and returns the same `BitError`;
and an LSB-first, little-endian message that ends inside a byte, whose `write_with` field
errors or panics five bytes in, leaves the buffer untouched and usable. Nineteen hand
mutants detected, fourteen on the writer (the tail's offset in `len` and `or_into`, the
width check and its position, both position overflows, each cursor not advanced, bit
and byte order, an unaligned run or tail, the wrong tail lent), three on keep and drop,
and two on the codec (the default layout, a message never kept). Three of them hang a test
rather than fail it; the runner's time cap catches those. 888 tests with all features across
both crates, 794 with default; clippy in four feature sets; MSRV 1.85 and the 32-bit
`no_std` build.

**Independent review.** No blocking finding. A differential harness with `encode_to_vec`
as the oracle agreed on bytes, the whole `BitError`, the `io::Error` kind and rollback
over 160,000 random `Sink` scripts in four layouts and 60,000 generated `#[bin]` messages
(LSB and little-endian, nested, padding, alignment, `write_with`, count prefixes, endings
inside a byte, consecutive frames), on seven buffer states (empty, exact, spare capacity,
after `advance`, `split_to`, `split` and `split_off`). `scratch()` is `None` on both
paths, and no generated encoder takes a concrete `BitWriter`. Applied: the rollback moved
into `Drop`, with the panic test; this record and the capacity note; the failing-encode
test compares the whole error; an LSB message and a failure at a non-zero position through
the codec; `emit_bits`'s doc.

### Phase 3: inline typed writes (2026-11-12)

The mirror of phase 2 on the encode side. `Sink::write`, the call a generated encoder
makes for each scalar field, was out of line on `BitWriter`: a call per field, the byte
order applied at run time, then one push per byte.

- **`BitWriter` and the in-place frame writer override `write`**, forced inline. A value
  that is a whole number of bytes, written at a byte-aligned cursor, is one
  `extend_from_slice` of its big-endian or little-endian bytes: by the effective byte
  order alone, whatever the bit order, for the reason given for reads. An aligned cursor
  is always the end of the buffer on these writers (every write keeps the buffer's length
  at the cursor rounded up; a debug assertion states it). Any other write inlines the old
  body, `write_by_bits`, which the trait's default also calls. The position bound is
  checked first and reported as `write_bits` reports it.
- `BytesWriter` forwards `write` to the `BitWriter` it wraps, and the public inherent
  `BitWriter::write` forwards to the override.

Instructions per encode, base and candidate; every decode mode is instruction-identical
bar `records_1024`, which reads 24 lower in 108,734.

| Mode | Base | Candidate | Cycles |
|---|---:|---:|---|
| `fixed44_encode` | 758.98 | 364.98 | 164.8 → 94.0 |
| `packet_encode` | 547.96 | 327.96 | 125.1 → 87.8 |
| `ipv4ish_encode` | 819.96 | 485.96 | 182.6 → 110.9 |
| `envelope_encode` | 351.98 | 297.98 | 92.4 → 86.7 |
| `records_encode` | 273,933.38 | 82,384.85 | 65,756.4 → 17,447.8 |
| `codec_encode` | 1,069.98 | 236.98 | 180.9 → 61.0 |
| `dmr_encode` | 1,998.92 | 1,998.92 | 427.9 → 429.2 |

`codec_encode` is now below `fixed44_encode`, the same message through `to_bytes`: the
codec appends to a buffer it already has, and `to_bytes` allocates one. `dmr_encode` does
not move: its fields are not whole bytes.

Criterion, three alternating pinned rounds against the parent, lowest median (ns); the
machine carried unrelated load during the run.

| Benchmark | Parent | Candidate | Ratio |
|---|---:|---:|---:|
| `fixed_44/encode` | 34.51 | 19.82 | 0.57 |
| `ipv4ish_message/encode` | 37.09 | 23.10 | 0.62 |
| `byte_aligned/encode` | 27.08 | 19.51 | 0.72 |
| `sub_byte/lsb_encode` | 79.52 | 67.49 | 0.85 |
| `sub_byte/msb_encode` | 81.44 | 71.08 | 0.87 |
| `counted_u16_1024/encode` | 81.47 | 71.64 | 0.88 |
| `counted_u32_256/encode` | 55.50 | 53.63 | 0.97 |
| `dmr_frame/encode` | 87.77 | 85.92 | 0.98 |
| `byte_runs_2x255/encode` | 22.28 | 22.56 | 1.01 |

The two hand-written baselines read 0.98 and 1.00. Chained with the size hint's step
(91 → 38 ns), `ipv4ish_message/encode` is at about a quarter of where phase 3 began; that
is the product of two parent-relative runs, not a measurement against the 0.10.0 tag.

What pays: code size. The probe's `.text` grows 409,267 → 415,811 bytes. Nothing pays in
instructions or cycles in any tier. Not measured: `BytesWriter`, which no probe mode or
benchmark encodes through; its `write` is a plain `#[inline]` forward, where
`BytesReader::read` was measured and forced.

**Tests.** `write_appends_whole_bytes_exactly_as_the_bit_path_does` writes 24 types (1 to
128 bits including every whole-byte width, a 24-bit type declaring each byte order, a
type whose `into_bits` sets bits above its width, zero-width and 136-bit) in all four
layouts, after 0 to 17 leading bits followed by nothing, a byte run, a typed write or a
lent tail, on a cloned writer, through the trait and the inherent method on `BitWriter`,
through `BytesWriter` and through the frame writer behind existing content, against
`write_by_bits`: result, cursor and bytes. `a_typed_write_refuses_a_position_it_cannot_count`
pins the bound from a byte boundary and from inside a byte, and that a too-wide value is
refused for its width first; the frame writer's bound test gained the typed write.
Fourteen hand mutants killed (each byte order's slice taken from the wrong end, the
orders swapped, the type's own order ignored, the fast path taken unaligned, too wide or
for a sub-byte width, the overflow's position, each writer's position wrapping or not
advancing, the frame writer's byte order, the byte order dropped from the general path).
890 tests with all features across both crates, 796 with default; clippy in three
feature sets; MSRV 1.85 and the 32-bit `no_std` build.

**Independent review.** No blocking finding; the equivalence was not refuted. The
reviewer derived the byte identity from `apply_byte_order` and `emit_bits` for all four
layouts, read every mutator of both writers to confirm an aligned cursor is the buffer's
end, and ran old, new and an independent bit-by-bit model over 20,000 scripts (230,717
typed writes, 59,292 on the fast path) interleaving clones, scratch, byte runs, lent
tails, alignment and padding: identical. Applied: the debug assertions tying the cursor
to the buffer (a position poked past the buffer used to panic in `emit_bits` and would
have appended quietly) and on the helper's precondition; this record and the §10 and
roadmap text; the test's prefixes, clone and widths; the error precedence; `emit_bits`'s
doc. Noted: on an aligned overflow the new path does not call `into_bits`, which only an
implementation with side effects could observe.

### Phase 3: `MessageStream` keeps its write buffer (2026-11-12)

The last encode item. `MessageStream::write_message` encoded each message into a fresh
`Vec` that grew by doubling (the generic encode has no size to reserve) and handed it to
the stream: four allocations for a 44-byte message, every message.

- `encode_into` (crate-private, with `net`) encodes into a `Vec` the caller keeps: it
  takes the allocation, empties it, and hands it back holding the message.
- `MessageStream` has a private `out` buffer, empty until the first write. A message is
  encoded into it and written from it only if the encode succeeded, so an encoder that
  fails after writing sends nothing, as before.
- **Retention is by message length, capped at the read scratch's 4 KiB.** After a message
  of at most 4 KiB the buffer is kept, cut back to 4 KiB if encoding grew it past that;
  after a longer one it is released. A connection therefore holds at most 4 KiB for
  writing, once it has written, on top of the 4 KiB read scratch it always held; one that
  only reads holds nothing new.

`stream_write_16x40` (sixteen 44-byte envelopes to a writer that keeps nothing) reads
740.17 → 179.11 instructions per message and 193 → 76–91 cycles; every other mode is
instruction-identical. No Criterion benchmark writes through a `MessageStream`.

**The bound was 64 KiB and keyed on capacity as first built, and review changed both.**
Keyed on capacity it released the buffer of every message from 32,773 bytes up, because
an unhinted encode doubles past 64 KiB on the way: such a message allocated exactly as
before, against a doc sentence that said otherwise. And at 64 KiB an idle connection that
had once written kept up to 65,536 bytes where it kept none (655 MB across 10,000
connections, in the reviewer's arithmetic). The length rule fixes the first; the 4 KiB
cap answers the second conservatively, by holding no more for writing than the type
already held for reading. The cost of that choice: a message over 4 KiB allocates as it
always did. Raising the cap is a one-line change and an owner call about memory per
connection, not a measured optimum.

The reviewer's counting allocator, old against new at the first bound, found no size at
which the new code makes more allocator calls: one call per four messages where there
were four, and equal counts past the bound.

Not done: `MessageDatagram::send_message` takes `&self` and still allocates per datagram;
a buffer there needs interior mutability or a signature change.

**Tests.** `net::component`: five sizes written in turn (longer, shorter, empty, longer)
arrive whole and in order from one buffer, which is not allocated before the first write
and does not move after it; a message in an LSB-first little-endian layout equals
`to_bytes`; an encode that fails before writing and one that fails four bytes in both
send nothing, return the error `to_bytes` returns, and leave the stream usable; a message
of exactly the bound keeps a buffer no larger than the bound and one byte more releases
it; a buffer doubled past the bound by a message under it is cut back and then reused;
and after a failed write the next message arrives alone. Ten hand mutants killed: the
buffer not emptied, not handed back, the default layout; a failed encode sent, the write's
error dropped; the bound off by one, removed, applied to every write, keyed on capacity,
and the cut-back removed. 896 tests with all features across both crates, 796 with
default; clippy in four feature sets; rustdoc; MSRV 1.85 and the 32-bit `no_std` build.

**Independent review.** No blocking finding. A differential against the old path on one
long-lived stream agreed over 200,000 rounds on bytes and the whole `BitError`, through
12,915 encode errors, 491 I/O errors and 1,002 panics in the encoder or the writer (after
an encoder panic the buffer is empty; after a writer panic it still holds the message
just sent, until the next call). `MessageStream` is not `Clone`, its auto traits are
unchanged, and `into_parts`, `try_into_inner` and `Debug` ignore the field. Applied: the
retention rule and bound above; a write after a failed write; the `Debug` comment; the
upgrade guide's sentence on what a stream holds.

With this, phase 3 is complete.

### Phase 6: two re-measurements on the phase 3 base (2026-11-12)

Neither is merged. Both were measured with the 54-mode probe against `6faf531d`.

**`#[cold]` on `BitError::in_field`: not taken.** The research row read as a gain before
phase 2 and was held back as "row 1's inlining arriving by accident". On the current base
it is a wash that fails the gate. It still helps some modes (`records_1024` 108,711 →
106,663 instructions, `ipv4ish_encode` 485.96 → 460.96, `codec_encode` 236.98 → 224.98,
`bytes_invalid` −10), and it costs gated ones: `records_encode` 82,385 → 85,460 (+3.7%),
`bytes_valid` +2, `dmr` +2, `unaligned_wide_tail` +2, `prefix_envelope`,
`prefix_eof_envelope` and `stream_16x40_whole` +1, `buffered_fixed44_1` +23 per message,
and the old buffered probe at one-byte chunks 1227.26 → 1231.26. The attribute moves
block placement across every generated decoder and encoder at once, with no mechanism
that favours the happy path in particular. Closed.

**The inline fast path on `IncrementalReader::read_into`: an owner decision.** The
researched change (an aligned run that is all there is copied inline, a `[u8; N]` field
at constant size; anything else goes out of line) still does what it did for a message
that is all there, and no longer does for one that is not:

| Mode | Tier | Base | Candidate | Cycles |
|---|---|---:|---:|---|
| `prefix_fixed44` | happy | 286.99 | 258.99 | 72.9 → 63.1 |
| `buffered_fixed44_whole` | happy | 424.06 | 396.05 | 124.6 → 102.4 |
| `buffered_fixed44_5` | expected | 3,886.11 | 3,884.77 | level |
| `prefix_short_fixed44` | expected | 615.99 | 619.98 | level (679–708M → 689–694M per run) |
| `buffered_fixed44_1` | expected | 18,202.42 | 18,307.90 | +7.5% (2,550–2,562M → 2,744–2,758M per run, three rounds) |

Every other mode is instruction-identical. So a fixed message carrying a byte array is
about 7 to 10% cheaper to decode from a prefix or a buffer when it is whole, and a
connection fed one byte at a time pays 0.6% more instructions and a repeatable 7.5% more
cycles on that message: each of the sixteen attempts that end inside the array now makes
the lend check inline and then the call, and the decoder's layout shifts with it. Before
phase 2 the same change read −80 instructions per message on that mode; the base it was
researched on had the array's read behind two calls. A second shape, which sends a miss
to the old out-of-line body unchanged, was worse (+2.0% instructions on
`buffered_fixed44_1`, +1.3% and +12% cycles on `buffered_fixed44_5`).

Under the happy-path rule this trade reads as worth taking: the mode that pays is the
most fragmented feed there is, and five-byte chunks are level. It is still a repeatable
cost in a gated tier, which the gate reserves for the owner.

**Owner decision, 2026-11-12: take it.** Merged as measured above (the final build reads
the same instruction counts). `unit::the_incremental_reader_fills_a_buffer_as_the_per_byte_loop_does`
reads 0 to 7 bytes at every cursor of a six-byte input in both bit orders, against the
per-byte loop run through the incremental reader itself: the result or the wait and its
position, the cursor, and the bytes written before a wait. Four hand mutants killed (the
copied run not written, a miss reported as success, a short read left as end of input,
the lend one byte short). 897 tests with all features across both crates, 797 with
default; clippy; MSRV 1.85 and the 32-bit `no_std` build. No separate review: twenty
lines whose equivalence the differential states directly.

### Phase 6: `BitBuf` compaction on unbounded buffers (2026-11-12)

**Owner decision, 2026-11-12: take it.** As researched. `BitBuf::push` reclaimed the
consumed prefix (one `memmove` of everything unread) whenever the new bytes would
otherwise make the buffer grow. A consumer that had fallen behind therefore moved its
whole backlog on every push, once the allocation fitted that backlog: the cost of a push
grew with how far behind the consumer was.

An unbounded buffer now reclaims only once the consumed bytes are at least as many as the
unread ones, and grows otherwise. Each byte moved is then paid for by a byte reclaimed,
whatever the backlog. A bounded buffer cannot grow, so it still reclaims whenever new
bytes would not fit; it also reclaims, as before, once consumed bytes match unread ones.

The probe gained three modes (57 with `tokio`): one message pulled and one pushed over a
buffer holding 100 or 1,489 unread 44-byte messages, unbounded with an allocation that
fits the backlog exactly, and bounded at that size.

| Mode | Base | Candidate | Cycles |
|---|---:|---:|---|
| `backlog_100` | 581.96 | 334.41 | 280.4 → 96.2 |
| `backlog_1489` | 3,449.84 | 333.93 | 2,889.1 → 99.3 |
| `backlog_bounded_100` | 587.96 | 588.96 | 283.4 → 282.7 |
| `buffered_fixed44_whole` | 396.05 | 389.05 | level |
| `buffered_fixed44_1` | 18,307.89 | 18,300.89 | level |
| `stream_16x40_1` | 6,526.77 | 6,519.76 | level |

The buffered and stream modes read 3 to 7 instructions lower; every other mode is
identical. The cost of a push no longer depends on the backlog: 334 instructions at 100
messages behind and at 1,489.

**What pays.**

- Memory under backlog. An unbounded buffer holds its consumed prefix until that prefix
  matches the unread bytes, so it reaches just under twice the backlog in length, and
  its allocation, which only ever grows, up to twice that again after a doubling. Before,
  it stayed at the backlog's size. It reallocates once to get there.
- `backlog_bounded_100` gains one instruction (the bound is tested once more). A bounded
  buffer kept nearly full still moves its backlog on each push: there is no fix short of
  a ring or twice the storage, and the type's documentation now says to give it room for
  twice the backlog if that matters.
- Two documented statements changed: the type's "reclaims them in place (one memmove,
  only when it avoids a reallocation)" and `push`'s "reclaiming ... when useful" now state
  the rule above.

**Tests.** `component::bitbuf`: 1,000 steps over a 100-message backlog decode every
message in order, hold under twice the backlog plus one push throughout, and peak at
exactly 1,188 bytes, which is where the consumed prefix first matches the unread bytes
(so the backlog was not slid on each push, and not left to grow further); the consumed
prefix is left alone at one consumed to three unread and reclaimed at two and two; a
bounded buffer reclaims to fit a message that would not otherwise fit, and leaves the
prefix alone when there is room. Six hand mutants killed (the old rule, never reclaiming,
strictly more consumed than unread, half as many, a bounded buffer reclaiming on every
push, a bounded buffer not reclaiming to fit). 900 tests with all features across both
crates, 800 with default; clippy; rustdoc; MSRV 1.85 and the 32-bit `no_std` build. No
separate review: one condition, researched and measured twice, with the owner's decision
on its memory cost.

### Phase 4: a fixed-length message decodes from one window (2026-11-13)

**Owner decisions.** 2026-11-12: build it and measure; a message with a wide unaligned
field skips the window by a compile-time rule, not by slack bytes or an opt-in.
2026-11-13: merge with the expected-tier cost recorded below.

**The ceiling.** A throwaway experiment (not committed) on the phase 3 base, instructions
per decode with cycles in brackets:

| How the message is decoded | `Fixed44` | `SubByte` (18 fields of 1 to 7 bits) |
|---|---:|---:|
| `decode_exact` on the base | 226 (46) | 692 (111) |
| Generated decoder over a window of constant length, called as a function | 208 (37) | 669 (105) |
| Field reads written into the caller, ordinary reader | 176 (33) | 512 (80) |
| Field reads written into the caller, window of constant length | 55 (18) | 103 (33) |

A window of constant length buys nothing by itself. The gain needs the reader's length to
be a constant and the field reads to sit in the same function as that reader, so the
per-field bounds and alignment checks fold.

**Mechanism.** The generated `bit_decode` of a fixed-length message starts with an attempt
that the old body follows unchanged:

1. `fixed_window` asks the source for `aligned_run(N)`, `N` being `BIT_LEN` rounded up to
   bytes, and wraps the lent `&[u8; N]` in a `BitReader` with the source's bit order, byte
   order and depth budget. A source that does not lend, is not byte-aligned, or holds
   fewer than `N` bytes returns `None`, and the old body runs.
2. The same magic check and field reads are emitted a second time, as text, against that
   reader, in a closure that returns the fields as a tuple.
3. On success the source's cursor is settled at the start plus what the window actually
   consumed, and `Self` is built once from the tuple.
4. On any failure the cursor is put back and the old body runs from the start.

Every error is therefore the field-by-field body's, with its kind, field path and offset.
`FixedBitLen::BIT_LEN` only sizes the window: a hand-written wrong one costs a wasted
attempt (too large: the window is not lent as often; too small: the window runs short and
is discarded), never a wrong value or cursor. The roadmap's macro-only marker for trusting
the length was not needed. Generated code carries no inline hint; `fixed_window` is
`#[inline(always)]` in the runtime, measured.

**The rule.** The first build put every fixed-length message through the window and made
wide unaligned fields slower (`dmr` +20% instructions, `unaligned_mid` +32%): a window is
exactly the message's length, so a 20-bit or 108-bit field near its end loses the 16
bytes of slack the phase 1 wide read relies on. `FixedBitLen` gained a hidden, defaulted
associated constant, `__BNB_WINDOW`, which is additive: a hand-written impl gets
"unknown". Leaf types (`u8` to `u128`, `UInt<T, N>`, bit enums and bitfields, `WireLen`)
report "leaf"; a message reports the result of the const fn `window_kind` over its
fields' widths and kinds in order:

- a leaf suits a window if it is at most 8 bits, or is whole bytes starting on a byte
  boundary;
- a nested message suits it if it suits a window itself and starts on a byte boundary;
- anything else, including an unknown kind, does not, and neither does the message.

The magic and byte arrays count as leaves; fields that take no input (`ignore`, `calc`)
are left out. The attempt is guarded by that constant, so a message the rule leaves out
compiles to the old body alone, and `dmr` and the `unaligned_*` modes are identical to
`main`. A message with a `dbg` field emits no constant: its trace would otherwise print
twice, and from a parent's window at an offset inside that window.

**Measurements.** Rust 1.99.1 (the machine's `stable` moved to 1.100.0 during this phase;
every table in this section is 1.99.1), against `main` at `00af68ca` with the probe's new
mode, instructions per decode. The probe gained `fixed44_rejected` (58 modes with
`tokio`): the 44-byte message with an `assert` on its sixth field that fails.

| Mode | Tier | `main` | Candidate | Cycles |
|---|---|---:|---:|---|
| `fixed44` | happy | 225.99 | 121.99 | 47.0 → 40.2 |
| `packet` | happy | 184.99 | 115.99 | 42.0 → 38.0 |
| `sub_byte` | happy | 691.98 | 280.98 | 110.5 → 87.1 |
| `records_1024` | happy | 108,709.41 | 61,606.43 | 17,204.2 → 10,999.4 |
| `prefix_fixed44` | happy | 258.99 | 140.99 | 63.9 → 51.0 |
| `limit_fixed44` | happy | 308.98 | 152.98 | 74.4 → 41.2 |
| `limit_prefix_fixed44` | happy | 341.98 | 185.98 | 93.3 → 55.1 |
| `buffered_fixed44_whole` | happy | 389.05 | 271.05 | 102.7 → 91.1 |
| `backlog_100` | happy | 334.41 | 216.41 | 96.0 → 86.1 |
| `backlog_bounded_100` | happy | 588.96 | 470.96 | 282.6 → 269.1 |
| `bytes_reader_fixed44` | happy | 271.98 | 199.98 | 53.7 → 53.6 |
| `bytes_reader_sub_byte` | happy | 772.98 | 150.98 | 123.0 → 57.8 |
| `buffered_fixed44_5` | expected | 3,879.27 | 3,882.39 | 1,253.3 → 1,260.2 |
| `buffered_fixed44_1` | expected | 18,300.89 | 18,790.89 | 5,479.1 → 5,635.7 |
| `prefix_short_fixed44` | expected | 619.98 | 633.98 | 169.5 → 172.4 |
| `limit_prefix_short_fixed44` | expected | 836.96 | 851.96 | 198.9 → 205.2 |
| `fixed44_short` | terminal | 552.96 | 566.96 | 137.9 → 142.3 |
| `limit_fixed44_short` | terminal | 786.96 | 793.96 | 199.7 → 202.6 |
| `fixed44_rejected` | terminal | 295.98 | 344.98 | 109.8 → 109.4 |
| `fixed44_trailing` | terminal | 226.98 | 122.98 | 40.2 → 34.0 |
| `limit_fixed44_long` | terminal | 314.96 | 163.96 | 71.9 → 45.8 |

Every other mode is identical in instructions, `records_encode` aside (6 fewer in
82,384): variable-length messages, the magic and dispatch modes, the other encode modes,
and the stream modes (the stream readers do not lend).

**What pays.**

- Expected tier. An attempt that is short asks for the window, is refused, and decodes
  field by field as before: 14 to 15 instructions per attempt. A 44-byte message fed one
  byte at a time pays it 43 times, +490 instructions (+2.7%, cycles +2.9%); fed in
  five-byte pieces, +0.1% (cycles +0.5%). An earlier run of the same code read +8.7% and
  +7.9% in cycles on those two modes; the instruction counts were the same. Owner
  accepted, 2026-11-13.
- A rejection decodes twice. A magic that does not match or an `assert` that fails is
  met in the window first, then again field by field for its error: `fixed44_rejected`
  +49 instructions (+16.6%), cycles level. The bound is one more decode of the fields
  before the failure. Terminal tier.
- What ran before a failure in the window runs again: user expressions in `assert`,
  `calc` and `magic`, and the `Drop` of a field already built. The guide's `assert` entry
  now says so and asks for expressions free of side effects.
- Code size. Each message the rule admits carries two bodies: the probe's `.text` goes
  424,419 → 433,123 bytes. Every fixed-length message's generated code holds the second body
  for the compiler to discard where the rule says no; build time was not measured.
- `Source::aligned_run` gained a documented requirement: a source that lends must be able
  to seek back over what it lent. Every lending source in the crate could already.

**Open.**

- The distance to the ceiling: `sub_byte` 281 against 103, `fixed44` 122 against 55. Not
  chased. The first build measured `fixed44` 106 and `packet` 107; the reviewed one
  measures 122 and 116. The difference was not isolated; the changes between them are the
  rule's guard, which is a constant, and the tuple the window returns.
- A per-type way to turn the window off, for a type its author knows arrives in pieces.
  The owner asked whether the macro can be told what to expect; it can, by a `#[bin]`
  option, which is additive. Its spelling is the owner's decision and is not in this
  change.

**Tests.** `component::window`, each decode compared with the same type behind a wrapper
that never lends: the rule admits what a window serves and nothing else (sub-byte fields
anywhere, whole-byte fields only on a byte boundary, nested messages, the magic, unknown
kinds); a message that suits a window asks its source for one lend and one that does not
asks for none; a window decodes what field by field decodes on every input, valid, short
and rejected, with the same value, error and cursor, through `BitReader`,
`IncrementalReader`, `BitBuf`, `BytesReader` and `LimitedSource`; a declared width that is
too small or too large only sizes the window; a message with its own `Drop` and a field
that is not `Copy` compiles and is built once; a traced message and its parent ask
their source for no lend. `tests/bin_hygiene.rs`: a trait of the user's that gives every
type a by-value `bit_pos` does not place the cursor (the test fails with the generated
method call restored). 906 tests with all features across both crates, 817 with
default; clippy with both; `cargo fmt`; the library and the generated code of
`nostd-check` on Rust 1.85; the 32-bit `no_std` build under `stable`, since the pinned
1.99.1 has no `thumbv7em-none-eabi` target here. One compile-error snapshot
(`tests/ui/uint_width_backing.rs`) mismatches under the pinned toolchain, on `main` as
well, for want of `rust-src`; the container run checks it.

**Mutants.** Nine hand mutants of the runtime (the rule's three arms, the alignment
bookkeeping, the window's layout and depth) and eight of the generator, all killed. A
ninth generator mutant, a traced message taking its own window, survived because the
guard it removed was redundant with the missing constant; the guard was deleted.

**Independent review.** Two blocking findings on the first build, both fixed and tested
above: a message with its own `Drop` and a non-`Copy` field stopped compiling (the
generated code took `Self` apart), and a `dbg` field in a nested message was traced from
the parent's window at the wrong offset. The reviewer's own harness, the base's decoder
against the candidate's, agreed on 869,136 outcomes across the lending readers.

A second review, of the fixed build, found one more blocking defect: the generated code
read the window's cursor by method call, and a trait in the user's scope with a by-value
`bit_pos` for every type answers before the reader's own `&self` method. The value
decoded correctly and the cursor stayed where it started, so `decode_exact` reported
trailing bytes. The call now goes by path (`Source::bit_pos`), with the hygiene test
above. It also asked for the twice-run `Drop` to be documented, the rejection cost to be
measured (`fixed44_rejected`), and the traced test to count lends; all three are in. Its
differential against `main` covered 14 message shapes (magic, `assert`, `calc`, `ignore`,
nesting three deep, mixed bit orders, 0-bit and 12-bit messages) at about 160,000 decodes
each over five start offsets and four layouts, through every lending reader: identical
values and cursors. It confirmed both earlier fixes, and that `Source` is sealed, so no
third-party source can lend. Left as they are: a field named `__bnb_used` or
`__bnb_start` does not compile, and a user expression naming `__bnb_r` sees the window's
position; both names are inside the reserved prefix.

### Phase 5: the readers over `std::io::Read` ask for whole bytes together (2026-11-13)

`StreamBitReader` and `BufSource` kept the per-byte defaults of `Source::read_bytes` and
`read_into`, and `StreamBitReader::read_bits` called `read_exact` on one byte at a time:
row 8 of the review. The probe gained eight modes for them (66 with `tokio`): a 44-byte
fixed message, an envelope with a 64-byte body and one with a 4 KiB body, each decoded
again and again from a reader that serves the same message without end, and an envelope
cut one byte short. On the base a payload byte cost about 180 instructions through
either reader, and a 44-byte fixed message 6,575 against 122 from a slice.

**What changed.**

- `StreamBitReader::read_bits`: a width of whole bytes read from between two bytes asks
  the reader for all of them (`pull`: `Read::read` until the buffer is full, an
  interrupted read tried again, the end of the input or an error stopping it). Anything
  else takes the old loop, kept as `read_bits_per_byte`.
- `StreamBitReader::read_bytes` and `read_into` from between two bytes: the same, into
  the caller's buffer or into a `Vec` grown a step at a time.
- `BufSource::read_bytes` and `read_into` from a byte boundary: the run is retained by
  one `fill_to` and copied out once (`take_run`).
- A step is at most 4 KiB before any of the run has arrived and then as much again as
  has (`RUN_STEP`), so a count read off the wire sizes neither an allocation nor a
  request. `BufSource::fill_to` takes the same bound.

**What stays as it was, by construction and by test.** The values; the error kind and
its offset; the reader's own state after a success and after a failure; exactly which
bytes, and which of the reader's errors, were consumed; what `BufSource` retains and its
end-of-input flag; where a run stops at `BufSource`'s cap (`BufferFull`), at the end of
the input, and where the position can count no further: a run reads the bytes the
position can still count and then reports `PositionOverflow`, as the per-byte loop did,
so the review's concern about `PositionOverflow` mid-run does not arise. A run of no
bytes reads nothing, not even what a forward seek skipped.

**What changes.** Owner decision, 2026-11-13: all three approved.

1. How the reader is called. Whole bytes from a byte boundary reach the reader as
   `Read::read` with room for what the field still needs. Before, `StreamBitReader`
   called `Read::read_exact` on one byte, and `BufSource` asked for a run one byte at a
   time (it already asked for a whole value at once). Two kinds of reader can tell:
   one that overrides `read_exact` to behave differently from a loop over `read`, and
   one whose `read` depends on the length it is lent (a datagram socket that drops what
   does not fit, a descriptor that refuses less than one record). The second kind lost
   all but the first byte of each datagram before.
2. `read_into` after an error. A reader is now lent the caller's buffer, so one that
   writes past what it reports leaves that there. The documentation now says the
   buffer's contents after an error are unspecified. Error and offset are unchanged.
3. Two failures that become answers. An uncapped `BufSource` sought far past what it
   retains and then read tried to allocate the whole gap (the reviewer saw 1 TiB abort);
   it now fills in steps and reports `UnexpectedEof`. A reader that claims more bytes
   than it was lent overflowed or panicked; the claim is now held to what was lent.

**Measurements.** Rust 1.99.1, against the probe commit `a297775a`, instructions per
decode:

| Mode | Tier | Base | Candidate | Cycles |
|---|---|---:|---:|---|
| `stream_reader_fixed44` | happy | 6,574.95 | 933.95 | 1,389.4 → 208.3 |
| `stream_reader_envelope` | happy | 13,054.95 | 489.95 | 2,436.7 → 156.9 |
| `stream_reader_payload_4k` | happy | 745,045.81 | 963.79 | 136,323.2 → 446.3 |
| `stream_reader_short_envelope` | terminal | 7,146.90 | 525.90 | 1,339.7 → 143.8 |
| `buf_source_fixed44` | happy | 4,844.95 | 1,954.95 | 1,045.6 → 403.6 |
| `buf_source_envelope` | happy | 14,838.95 | 667.95 | 2,773.8 → 133.0 |
| `buf_source_payload_4k` | happy | 875,854.04 | 1,368.81 | 155,785.2 → 566.7 |
| `buf_source_short_envelope` | terminal | 14,566.90 | 1,119.90 | 2,789.8 → 227.0 |

The other 58 modes are identical. Nothing pays in any tier; the probe's `.text` goes
447,507 → 456,179 bytes.

**Not covered.** A read that starts or ends inside a byte still takes the stream one
byte at a time, on both readers. A `LimitedSource` window over either reader still reads
its runs per byte (it builds them on `aligned_run`, which these readers cannot lend).
`BufSource` still makes one reader call per typed field.

**Tests.** `component::stream_runs`. Two differentials, 4,000 random programs each: ten
steps of byte runs of every size (a count no input could back among them), bit reads of
0 to 136 bits, seeks and `compact`, in the four layouts, over a scripted reader that
hands over a fixed or a varying amount per call and, once each at chosen offsets,
interrupts, refuses (`WouldBlock`), fails, or reports `UnexpectedEof` as an error; one
case in eight starts where the position is about to stop counting; `BufSource` caps from
0 to `usize::MAX`. One reader goes through its own reads and its twin through the old
per-byte code (`read_bits_per_byte`, verbatim, for the forward reader); after every step
the outcome, the partial fill and the whole state including the scripted reader's must
match. Each differential also counts how its runs ended and fails unless every ending
was met often (success, the end of the input, a reader error, the cap, the position
bound). Four more: a run and a whole-byte value are asked of the reader in one call, a
long run in growing steps; a count nothing backs sizes neither the request nor what is
held; a reader that trickles one byte a call is not lent a freshly cleared step for
every read; a reader that claims more than it was lent is held to what it was lent.
912 tests with all features across both crates and 823 with default, each beside the
one known snapshot failure; clippy with both; rustdoc with warnings denied; the
`no_std` check; Rust 1.85.

**Mutants.** 34 by hand, all killed, none by a timeout: the conditions that choose the
new paths (never taken, taken inside a byte, taken for any width, for a width above 128
bits); the byte order a value is assembled in and where in the buffer it is read; the
position and cursor after a success and after a short run; each error's kind and where
it is reported; an interrupted read; the end of the input against a reader's error; the
position bound; the empty run after a seek; where a run lies in what is retained; the
copy out; the step sized by the count, and the step that does not grow, on both
readers; a step cleared again for every read; a reader's count taken at its word.

**Independent review.** One blocking finding, fixed: `BufSource::fill_to` cleared a whole
step before every `read` and cut it back to what came, so a reader that returns less
than it is asked made a run quadratic (1 MiB at one byte a read 8.2 ms → 1.65 s, 64 MiB
at 1,460 bytes a read 0.50 s → 6.56 s, measured by the reviewer; the forward reader was
linear throughout). A step is now cleared once and filled by as many reads as it takes,
with the trickle test above. Also from the review: the reader-length sensitivity in
item 1; item 2; both parts of item 3; a whole-byte width above 128 bits, whose guard no
test reached, now in the differentials; wider generators (varying read sizes, events on
step boundaries, caps of 0, 1 and around a step, seeks to byte boundaries). The
reviewer's own differential against the parent commit through the public API, 400,000
programs per reader, was identical in outcomes and state.

### Phase 5: a large message is read straight into the buffer (2026-11-13)

`net::read_message` and `read_message_async` read a scratch at a time and copied each
piece into the `BitBuf`. `MessageStream`'s scratch is 4 KiB, so a 1 MiB message took 256
read system calls. The research prototype was rebuilt on this base with an async path.

**Owner decisions, 2026-11-13.** Direct reads are the default. A larger scratch is an
option on top, not the alternative: `MessageStream::with_read_scratch(len)` (the first of
three shapes offered; a read-policy enum with a way to turn direct reads off, and no new
API, were the others). The cost to readers outside `std` below was shown with the choice.

**What changed.**

- A message known to need at least `scratch.len()` more bytes, and at least 4 KiB
  (`DIRECT_FLOOR`), is read straight into the buffer's storage: exactly what its hint
  asks for, or as much as a bounded buffer has room for, with no read-ahead. Smaller
  needs go through the scratch as before.
- Sync: `Read::take(reader, want).read_to_end(storage)`. Async: room is reserved a step
  at a time (a scratch before anything has come, then as much again as has) and
  `take(step).read_buf(storage)` appends what arrives. Neither sizes an allocation or a
  request by the hint, which a length off the wire may have set.
- `BitBuf::make_room_in_place` makes the room `push` would and says whether the cursor
  moved. If it did, nothing is read and the message is decoded again, the rule the
  scratch path already had for a rebased cursor.
- After a direct read the message is always decoded again: the read carries no count of
  what is still missing.
- `MessageStream::with_read_scratch(len)` sets the scratch length, 4 KiB otherwise; zero
  is taken as one.

**What changes for a caller** (§12.1's "each read is bounded by scratch and free
retained capacity"): a read for a large message is bounded by its hint and the capacity,
not by the scratch; that read takes nothing past the message, where the scratch reads
ahead; the scratch length is also the size from which a message bypasses it. Unchanged:
`BufferFull` before another read, the empty scratch, `Interrupted` retried, the
transport's own error kept, accepted bytes kept across an error, one finite attempt at
the end of the input, and, for the async form, exactly the accepted input kept when the
future is dropped while a read is pending.

**Measurements.** Wall time per message through a `MessageStream`, best of three rounds,
Rust 1.99.1, a throwaway harness (kept on the measuring machine, not committed), ns:

| Reader | Size | `main` | Candidate | Ratio | Read system calls |
|---|---:|---:|---:|---:|---|
| Unix socket | 4 KB | 1,637 | 1,630 | 1.00 | 1.0 → 1.0 |
| Unix socket | 16 KiB | 4,552 | 3,736 | 0.82 | 4.0 → 3.0 |
| Unix socket | 64 KiB | 17,265 | 7,308 | 0.42 | 16.0 → 3.2 |
| Unix socket | 1 MiB | 268,454 | 115,110 | 0.43 | 256.0 → 15.8 |
| `&[u8]` | 16 KiB | 586 | 488 | 0.83 | |
| `&[u8]` | 64 KiB | 2,164 | 1,762 | 0.81 | |
| `&[u8]` | 1 MiB | 38,852 | 31,761 | 0.82 | |
| Outside `std`, fills every request | 16 KiB | 589 | 689 | 1.17 | |
| Outside `std`, fills every request | 64 KiB | 2,185 | 2,497 | 1.14 | |
| Outside `std`, fills every request | 1 MiB | 39,577 | 47,388 | 1.20 | |
| Outside `std`, 1,460 bytes a call | 16 KiB | 639 | 644 | 1.01 | |
| Outside `std`, 1,460 bytes a call | 64 KiB | 2,332 | 2,262 | 0.97 | |
| Outside `std`, 1,460 bytes a call | 1 MiB | 43,124 | 44,638 | 1.04 | |

Messages of 4 KB read 0.92 to 1.00 from every reader. The probe gained three modes (69
with `tokio`), whose reader is of the kind outside `std`; instructions per message
against the probe commit `d321577d`:

| Mode | Tier | Base | Candidate | Cycles |
|---|---|---:|---:|---|
| `stream_payload_64k_whole` | happy | 12,516.93 | 11,481.92 | 10,108.6 → 12,272.1 |
| `stream_payload_64k_1460` | happy | 17,315.99 | 16,470.98 | 12,195.5 → 11,540.3 |
| `stream_payload_1m_1460` | happy | 265,877.48 | 248,672.11 | 227,639.5 → 217,525.4 |
| `stream_16x40_whole` | happy | 318.85 | 312.41 | 114.4 → 109.9 |
| `stream_16x40_64` | expected | 524.22 | 509.66 | 208.0 → 191.0 |
| `stream_16x40_1` | expected | 6,519.73 | 6,236.73 | 1,313.5 → 1,224.8 |

**What pays.**

- A reader outside `std` that fills large requests: 1.14 to 1.20 of the time on
  messages of 16 KiB and up, and `stream_payload_64k_whole` +21% in cycles. `std` clears
  the buffer it lends a reader that does not implement `read_buf`, which no reader
  outside `std` can on stable Rust; the scratch path copied instead. Such a reader
  normally does work per byte (TLS, decompression) that this is small beside. There is
  no way to turn the direct read off; the owner chose the shape without one.
- Three `StreamBitReader` modes read 4 instructions more and `buf_source_short_envelope`
  8 (0.4% to 0.8%; cycles level or lower). Not this change's code: those decoders and
  readers are the same instructions in both binaries, compared function by function,
  so the difference is in what they call outside the crate.
- The probe's `.text` goes 457,299 → 462,099 bytes.

**Measured and not taken as the default.** A 64 KiB scratch for every `MessageStream`:
the same socket gain from 16 KiB up and 0.76 at 4 KB, for 60 KiB more held per
connection and 1.15 to 1.27 on messages of 64 KiB and up from any reader that fills
requests. Direct reads with a 64 KiB scratch, which is what `with_read_scratch(64 * 1024)`
gives: 0.84 at 4 KB, 0.42 at 16 KiB, 0.41 at 64 KiB, 0.43 at 1 MiB over the socket.

**Tests.** `net::component::direct`, twelve. A sync and an async differential (600 and
400 scripted transports: one to six messages of sizes around the scratch, sometimes cut
short, handed over 1, 7, 1,460 bytes or everything at a time, with interruptions,
refusals and resets at chosen offsets, into unbounded and bounded buffers): reading with
a scratch of 1, 16 or 4,096 bytes gives the messages, the errors and the bytes consumed
that reading through a 64 KiB scratch gives, which no message of the script outgrows.
A message at the threshold exactly and past it is read with nothing after it taken and
only the first read through the scratch; one byte under it reads ahead. A prefix
claiming 4 GiB with ten bytes behind it sizes neither the buffer nor a read, sync and
async. A bounded buffer takes what it has room for, reclaims a consumed message, and
does not grow, sync and async. Room made by moving the cursor voids the hint before
anything is read. A message that starts inside a byte. An async read dropped while
pending, inside a step, at a step's end and well on, keeps exactly what came, and the
message then completes. The scratch length can be set, reads ahead as far as it
reaches, and a short one does not send small messages direct. 925 tests with all
features across both crates, 876 with `mock` and 823 with default, each beside the one
known snapshot failure; clippy with
all features, default and `mock`; rustdoc with warnings denied; the `no_std` check;
the library on Rust 1.85.

**Mutants.** 20 by hand, all killed, one by a timeout (the end of the transport going
unnoticed loops): the threshold at its exact value, without the floor and without the
scratch's length; never direct, sync and async; the buffer's room ignored; no room made;
going on after the cursor moved, in the buffer and in the caller; a read that takes all
the transport has, sync and async; a decode skipped after a direct read; no room
reserved for an async step, a step sized by what is missing, and a step that does not
grow; an interrupted async read; the scratch length kept at zero and not set. A first
run let four through. Two were a count of what a full buffer left unread, which only
saved one decode attempt before `BufferFull` and is gone. Two were the threshold at its
exact value, where both ways take the same bytes: the tests now look at what the
scratch still holds. After the floor went in, three more got through until the cursor
test asked for a body large enough to go direct and the scratch test fed its message
piecemeal.

**Independent review.** One blocking finding: the snapshot `public-api.txt` lists
`MessageStream` twice, by its path and as a re-export, and the new method had been
added once. Fixed by hand; the pinned nightly that generates the file is not on the
measuring machine, so the CI job is its check. Taken from the review besides: the floor
(a caller with a 16-byte scratch saw a 64-byte message go 69–73 → 96–97 ns through a
direct read); the async read as `reserve` and `read_buf` in place of a zero-filled step
cut back by a drop guard, which a leaked future or an `AsyncRead` that replaces its
buffer could leave holding bytes the transport never gave; the differentials now
compare the bytes consumed, as their comment said; read-count assertions loosened where
they pinned `std`'s growth heuristics. Found sound: a bounded buffer cannot be pushed
past its capacity or reallocated, `read_to_end` keeps what arrived before an error, no
overflow and no new unbounded loop, and `BinCodec`, the datagram types and the raw
handoff are untouched.
