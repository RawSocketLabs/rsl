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
  `bits = msb|lsb` and `bytes = big|le` as independent knobs, and inferred /
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
their spec value and `calc` fields recomputed, always spec-compliant. The two differ only
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
| `SeekReader` | `Read + Seek` | yes (via `io::Seek`) | a large file/container |
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
are unchanged. This is a runtime-only additive release, expected as `0.4.1` with
macros remaining `0.4.0`. CI's source baseline advances to published `0.4.0` in
all/default/no-default feature modes with `--release-type patch`; release-plz still
selects actual versions.

Local verification (2026-09-07): eleven focused macro tests, seven builder unit
tests, doctests, workspace and bytes/mock/all-feature suites pass. Removing the
delegation makes the non-Clone unit test fail; restoring it passes. Strict bnb/macros
Clippy, configured workspace Clippy, denied-warning all-feature/no-default docs,
Rust 1.85 workspace/all-feature/renamed-consumer checks, and bare-metal compilation
pass. The pinned API snapshot is additive; all three 0.4.0 compatibility checks,
cargo-deny, benchmark smoke, and two million fuzz inputs pass. Hosted CI and the
generated release candidate remain delivery gates. Independent review found no
remaining issues after tightening compatibility checks to the patch release class.
The next smallest consumer change is SOCKS immutable method validation using the
consuming helper on copied methods; no protocol surface expansion is needed.

## 10. Performance

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

## 11. Incremental decoding and lossless handoff (0.5 candidate)

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
arbitrary callbacks. General variable-element parsing replays and can be quadratic under
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
| `cargo build --manifest-path bitsandbytes/bnb/nostd-check/Cargo.toml --target thumbv7em-none-eabi` | Pass on stable. The 1.85 bare-metal target is not installed locally; CI requires the checked 1.85 host plus stable bare-metal combination. |
| `RUSTDOCFLAGS='-D warnings' cargo doc` for both crates/all features and runtime/no defaults, `--no-deps` | Pass. |
| Pinned `cargo +nightly-2026-06-17 public-api` snapshot; historical `check-api-delta.sh` in `all`, `default`, `none` modes | Pass; exact reviewed delta guarded the 0.4 → 0.5 migration. The checker was retired after publication. |
| `cargo semver-checks -p bitsandbytes --baseline-version 0.4.0 --release-type major` in all/default/explicit-none modes | Pass, but major mode executes **zero** compatibility rules (254 skipped); it is not compatibility proof. The diagnostic patch run executes 223 rules: 222 pass, one fails for the intentionally removed `try_push`/`into_inner`. Both crates require 0.5.0. |
| `cargo package -p bitsandbytes-macros -p bitsandbytes --all-features --allow-dirty` | Both archives build together using Cargo's temporary registry. Current development manifest versions are not the release candidate versions; repeat on the generated release PR. |
| `cargo deny check`; `actionlint` | Pass with refreshed advisory data. Unused-license and duplicate-syn warnings remain; yanked wnaf is absent. |

Benchmark, allocation, mutation, fuzz, and final reviewer evidence below complement these
checks. A clean committed worktree must additionally pass `scripts/ci-act.sh pre-push`,
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
