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
constant width folds them down. A wider read that starts or ends inside a byte gathers the
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
arbitrary callbacks. **Recursion is unbounded:** a message type that contains itself (through
`Vec`, `Option` or `Box`) decodes by native recursion to a depth the input chooses, with no
depth limit, so hostile input can overflow the stack — an abort the caller cannot catch
(reproduced in the 0.9.0 review). Until a decode depth budget exists ([`ROADMAP.md`](ROADMAP.md)
§C), the caller bounds it: cap the frame size so that worst-case depth (one level per byte
consumed) times the per-level stack use fits the decoding thread's stack, or express the
recursive part with a `parse_with` codec that counts depth and returns an error. General variable-element parsing replays and can be quadratic under
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
and free retained capacity. Even huge hints fill remaining capacity before `BufferFull`,
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
not a silent larger budget. The user retained this strict gate.

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
