# bnb / bnb-macros

A fast, **owned bit-aware** binary codec, integer-backed, that collapses the
capabilities of a bitfield/int/enum stack (`modular-bitfield(-msb)`,
`bitfield-struct`, `bitbybit`, `arbitrary-int`, `num_enum`) plus a declarative codec
**modeled on `binrw`** into one crate. The unified `#[bin]` attribute is the
whole-message front-end
(magic/count/ctx/map/if/calc·temp/reserved/positioning/validate + a
`Source`/`SeekSource`/`BufSource`/`SeekReader` I/O ladder, opt-in `bytes`). The codec
is entirely in-house — `binrw` is an inspiration, not a dependency (see
`ACKNOWLEDGMENTS.md`); `DESIGN.md` has the design rationale.

> Canonical agent-guidance file for this crate; `CLAUDE.md` is a symlink to it, and
> the repo-root `README.md` is the user-facing overview.
>
> **Published as `bitsandbytes` / `bitsandbytes-macros`; imported as `bnb` / `bnb_macros`**
> (via `[lib] name`). Downstream: `bnb = { package = "bitsandbytes" }`, then `use bnb`.

## Two-crate layout (required)

- `bnb/` — runtime lib: `int` (`UInt<T, N>` + `u1..u127`), `field` (the `Bits`
  and `Bitfield` traits, `ByteOrder`/`BitOrder`), `error`, `builder`
  (`BuilderError`), `bitstream` (the codec runtime: `Source`/`Sink`,
  `BitReader`/`BitWriter`, `BitDecode`/`BitEncode`, the I/O ladder, the `bytes`
  adapters), and the crate root (re-exports + the `BitEnum` marker trait +
  `__private` for generated code).
- `bnb-macros/` — proc-macro crate: `#[bitfield]`, `#[derive(BitEnum)]`,
  `#[bitflags]`, `#[derive(BitsBuilder)]` (the `builder` module is shared by the
  standalone derive and the `#[bitfield]` intercept), the low-level
  `#[derive(BitDecode)]`/`#[derive(BitEncode)]` codec derives, and `#[bin]` (the
  unified codec attribute that folds those derives + the builder).

A proc-macro crate cannot also export runtime items, hence the split. Depend only on
the runtime (`bnb = { package = "bitsandbytes" }`); it re-exports the macros, so
downstream never names `bitsandbytes-macros` directly.

## How the `#[bitfield]` macro works (the load-bearing idea)

The macro **cannot** know the numeric width of a field whose type is another
bitfield/enum — those widths live in `<T as Bits>::BITS`, resolved by the
compiler. So `#[bitfield]` emits **const expressions** (`<T as Bits>::BITS`,
cumulative sums, offset/mask arithmetic) that the compiler evaluates during
const-eval; the generated accessors then shift/mask the single backing integer.
If you change the macro, keep that invariant — don't try to compute widths in
the proc-macro.

- Layout consts: `Name::__bits_w_<field>` / `__bits_off_<field>` /
  `__bits_mask_<field>` and `Name::WIDTH` (the inherent impl carries
  `#[allow(non_upper_case_globals)]`).
- `bits = msb`: `off = WIDTH - cumulative_including_field`. `bits = lsb`:
  `off = cumulative_before_field`. `#[bits(A..=B)]`: absolute (`off = A`).
- A `#[bitfield]` emits `Bits` + `Bitfield` impls (so it nests in another
  bitfield and in a `#[bin]` message) using its **declared** byte order, plus
  inherent `to_be_bytes`/`to_le_bytes`/`from_be_bytes`/`from_le_bytes`.
- `#[bitfield]` **intercepts `#[derive(Debug)]`** (like `BitsBuilder`, via
  `split_outer_attrs`) and emits a custom `Debug` over the *logical* getters
  (`version: u4(4), ihl: u4(5)`) instead of letting std derive the opaque
  `{ value: 69 }` on the collapsed backing int. A `#[bin]` struct's std `Debug`
  then shows nested bitfields decomposed.
- A byte-aligned `BitEnum` *also* gets `num_enum`-parity conversions
  (`bitenum.rs::conv_impls`): `From<Enum> for uN` always; `From<uN> for Enum`
  when there's a `#[catch_all]` (total) else `TryFrom<uN> for Enum` (errors with
  `bnb::UnknownDiscriminant`). So a magic-byte enum needs no hand-written
  `From`/round-trip test — the derive is the whole file. (`From`/`TryFrom` are
  mutually exclusive per enum, so the std blanket `TryFrom` never collides.) A
  sub-byte enum (`u4`) gets none of these — it is meaningful only nested in a
  `#[bitfield]`.

## `#[bitflags]` and `#[derive(BitsBuilder)]`

- **`#[bitflags(uN)]`** takes `bool` fields (an attribute macro needs a valid
  struct, and a `bool` *is* a 1-bit flag); each auto-assigns a bit by position
  (LSB-first), or `#[flag(N)]` pins it. It generates UPPERCASE consts, set
  operators, `contains`/`iter`, per-flag `fin()`/`with_fin`/`set_fin`, and
  `Bits`/`Bitfield` impls — so a flag set nests in a `#[bitfield]`. `from_bits`
  **retains** unknown bits (dual-use); `from_bits_truncate` drops them.
- **`#[derive(BitsBuilder)]`** — required-by-default; `build() -> Result<_, BuilderError>`
  errors on the first unset field; `#[builder(default)]` / `#[builder(default = expr)]`
  opts a field out. **Intercept mechanism (load-bearing):** because `#[bitfield]`
  collapses the struct to one integer *before* derives run, a real derive can't
  see the logical fields — so `#[bitfield]` itself scans its derive list for
  `BitsBuilder` (`split_outer_attrs`), generates the builder from the fields, and
  strips the marker. A real `BitsBuilder` derive also exists for **plain**
  structs. So: put `#[bitfield(...)]` **above** `#[derive(BitsBuilder, ...)]`.

## `#[bin]` — the unified whole-message codec

`#[bin]` is the crate's flagship: one attribute that
folds the codec (`BitDecode`/`BitEncode`) and the required-by-default builder over
a struct, generating `decode` (cursor over a `Source`), `decode_all`/`decode_iter`/`decode_exact`/
`peek` (slice/`Vec`, layout-baked), `encode`/`to_bytes`,
`Foo::builder()`, and a positional `Foo::new(fields…)` (every stored field, in order — the
struct-literal replacement, since a `reserved`/`calc` message's injected `encode_mode` field
can't be named in a literal). It reads/writes fields at **arbitrary bit offsets**, so the same
attribute handles byte-aligned headers and sub-byte frames alike.

- **Lowering.** `#[bin]` lowers to `#[derive(BitDecode, BitEncode, BitsBuilder)]`
  + `#[bit_stream(...)]`; the field-directive logic lives in those derives, which
  stay usable directly. `#[bin]` is a thin, zero-duplication front-end over them.
- **Struct-level options:** `read_only` / `write_only` (directional codecs),
  `no_builder`, `bit_order = msb|lsb`, `bytes = be|le` (`big`/`little`),
  `allow_byte_aligned`.
- **Struct-level wire mapping (`bin_struct_mapped`):** `map = |w: Wire| Self` /
  `try_map = |w: Wire| Result<Self, E>` (decode) + `bw_map = |s: &Self| Wire` (encode) make a
  *logical* struct serialize via a separate *wire* type — the whole-struct analogue of the
  field-level `map`. It **bypasses the field codec**: the struct's fields are logical data, the
  wire type owns the bytes. Generated impls delegate to the wire type (`decode_mapped_msg`/
  `decode_try_mapped_msg`/`encode_mapped_msg` runtime helpers), so the ordinary slice surface
  works at the wire layout, and it forwards `FixedBitLen` (so it nests; the **wire type must be
  fixed-length**). Wire type is read from the `map`/`try_map` closure's annotated param (or the
  `bw_map` return for a write-only type). Mutually exclusive with `magic`/`ctx`/`validate`/`tag`;
  struct only (not enums). See `guide::mapping`, `tests/bin_wire_map.rs`.
- **Field directives** (the inherited grammar): `#[br]`/`#[bw]`/`#[brw]` with
  `magic`, `count`, `ctx`/`args`, `map`/`try_map`, `if`, `calc`/`temp`,
  `reserved`/`reserved_with`, `parse_with`/`write_with`, `pad_before`/`pad_after`/
  `align_*`/`seek`/`restore_position`, and `assert`/`validate`. Positioning amounts
  use the `prelude` typed helpers (`4.bits()`, `3.bytes()`).
- **I/O ladder** (`bnb::bitstream`): `Source`/`Sink` (the bit cursors), the
  `SeekSource` marker for in-memory buffers, `BufSource<R: Read>` (bounded
  retain-and-seek over a forward-only reader), `BitBuf` (push/pull bit-aware in-memory
  buffer — pushable, a `SeekSource`, `no_std`; **reclaim is deferred + in place** so a push/pull
  loop reuses one alloc, and `BitBuf::bounded(cap)` + `try_push`/`grow` give a fixed alloc-once
  footprint, `CapacityError` on overflow), `SeekReader<R: Read + Seek>`, and —
  under the opt-in **`bytes`** feature — `BytesReader`/`BytesWriter` for async
  framing. Seeking is free cursor math; there is no uniform `Seek` requirement.
- **Opt-in transport helpers (all `std`).** `tokio`: `BinCodec<T>`, a `tokio_util::codec`
  Decoder/Encoder over `Framed` (TCP) and `UdpFramed` (UDP). `net`: `MessageStream`
  (`read_message`/`write_message` over any `Read + Write`; buffers reads with a `BitBuf` rather than
  re-rolling the loop) and `MessageDatagram` (`send_message`/`recv_message` over a `DatagramSocket`)
  — both decode in the message's own layout (bound `BitDecode + BitEncode`, to reach `T::LAYOUT`).
  **`DatagramSocket` is sealed** via a private `sealed::Sealed` supertrait: `bnb` impls it for
  `UdpSocket`/`UnixDatagram` (and `MockDatagramSocket` under `mock`) — a new impl needs
  `impl sealed::Sealed` too, so downstream can't (locked by `tests/ui_seal`). `mock` (implies `net`,
  for `[dev-dependencies]`): `MockDatagramSocket`/`MockStream` in-memory transports with scripted
  inbound, captured outbound, chunked delivery, and error injection (`fail_after`/`fail_next_recv`)
  — unit-test `net` code without a socket.

## `no_std` (Option A) — the `std` feature

`bnb` is `no_std` + `alloc`; the default-on **`std`** feature adds everything backed by
`std::io`. **Load-bearing facts when editing the codec/macros:**

- `alloc` is unconditional (`extern crate alloc` in `lib.rs`); use `alloc::{vec::Vec,
  string::{String, ToString}}` in the runtime, and emit `#bnb::__private::{Vec, String,
  vec}` from the macros — **never `::std::…` inside a `quote!`** (it breaks `no_std`
  consumers). Errors impl `core::error::Error`, not `std::error::Error`.
- **Runtime-crate path in generated code is resolved, not hardcoded.** Each macro fn
  that emits runtime paths does `let bnb = crate::bnb_path();` and interpolates `#bnb`
  (e.g. `#bnb::__private::Vec`) — **never a literal `::bnb` in a `quote!`**. `bnb_path()`
  (in `bnb-macros/src/lib.rs`) resolves it via `proc-macro-crate`: the crate is published
  as `bitsandbytes` but its lib is named `bnb`, and Cargo links any *non-renamed*
  reference (the crate's own tests/doctests/examples, `trybuild`'s temp crates, an
  un-aliased `bitsandbytes = "…"` consumer) by the **lib name `bnb`**, but a
  `package = "…"`-renamed dep by its **key**. So `crate_name("bitsandbytes")` returning
  the package name (or `Itself`, or not-found) ⇒ emit `::bnb`; any other name ⇒ that key.
  `lib.rs` carries `extern crate self as bnb;` so `::bnb` resolves inside the lib too.
  (If the package is ever renamed, update the `"bitsandbytes"` string in `bnb_path`.)
- Gate behind `#[cfg(feature = "std")]`: the reader/writer adapters (`StreamBitReader`/
  `BufSource`/`SeekReader`/`SourceReader`/`SinkWriter`, `as_read`/`as_write`),
  `encode_to_writer_with`, `From<std::io::Error>`, and `ErrorKind::Io`.
- **Two encode forms — verbatim vs canonical** ([`EncodeMode`]). `to_bytes` and `bit_encode`
  are **verbatim**: they emit exactly what's stored (retained `reserved`, stored non-`temp`
  `calc`) — never silently rewriting the caller's bytes, and `decode → to_bytes` is
  byte-identical. `to_canonical_bytes` and `canonical_bit_encode` are **canonical**: reserved
  → spec value, `calc` → recomputed, so the result is always spec-compliant. `canonical_bit_encode`
  is a **defaulted method on `BitEncode`** (`fn canonical_bit_encode(..) { self.bit_encode(..) }`),
  overridden by the derive **only** when a message has a `reserved` or non-`temp` `calc` field
  (else verbatim == canonical) — there is no separate `CanonicalEncode` trait. There is **no
  canonical decode** — `decode_*` is always verbatim. The same `reserved`/`calc` condition also
  generates the inherent `to_canonical_bytes` plus the in-memory helpers
  `to_canonical(self) -> Self`, `canonical_diff(&self) -> Vec<&'static str>` (fields differing
  from canonical), and `is_canonical(&self) -> bool`. (Sink-writing is the `BitEncode` trait
  methods `bit_encode`/`canonical_bit_encode`, not an inherent `encode_into` — that wrapper was
  cut as redundant.)
- **The mode is carried on the value, not passed to `encode`.** A `reserved`/`calc` message gets
  a wire-ignored **`encode_mode`** field (default `Verbatim`): builder `.encode_mode(…)`,
  `set_encode_mode`/`with_encode_mode`, getter `encode_mode()`. `BitEncode::encode_mode(&self)`
  (default `Verbatim`) is overridden to return it, and the `std`-gated blanket `EncodeExt::encode(w)`
  (no `mode` param) consults it to pick `bit_encode` vs `canonical_bit_encode`. `EncodeExt` is an ext
  trait — **not** a generated inherent method — because a proc-macro can't see the consumer's
  features, so a generated `#[cfg(feature="std")]` would key off the *wrong* crate's flag (bring it
  in with `use bnb::prelude::*`). **`#[bin]` injects the field and intercepts `Debug`/`PartialEq`/
  `Hash`** (custom impls over the user fields) so the mode is excluded from equality/hash/Debug — a
  render preference, not data — which means these types are **builder/`decode`-constructed** (the
  private field can't appear in a literal). Generated, portable (no `EncodeExt`) methods: `to_bytes`
  (verbatim) and `to_canonical_bytes` (canonical); sink-writing is the trait methods on `impl
  BitEncode { const LAYOUT; fn bit_encode; [fn canonical_bit_encode + fn encode_mode when
  reserved/calc] }`.
- `#[br(dbg)]` is `std`-only (`tracing` is an optional dep enabled by `std`); the
  `__private::tracing` re-export is `std`-gated.

### Low-level `#[derive(BitDecode/BitEncode)]` + the right-tool guard

The bare derives are the codec without the builder/`#[bin]` sugar — use them when
you want only read/write. They carry a **right-tool guard (don't remove):** a
const-eval assert (`alignment_guard`, same mechanism as the `#[bitfield]` fill
assert) that **rejects an all-byte-aligned struct** — every field width a multiple
of 8 ⇒ the cursor never leaves byte boundaries ⇒ `#[bin]` is the better tool, and
a sub-byte run that fills one integer wants `#[bitfield]`. The message names those
alternatives. Escape hatch: struct-level `#[bit_stream(allow_byte_aligned)]` (a
helper attr declared by both derives; `#[bin]` always sets it, since a byte-aligned
message is a first-class `#[bin]` use, not a misuse). Proof:
`tests/ui/bitstream_byte_aligned.rs` (reject) + `tests/bitstream_guard.rs`
(override). Grouping is steered by the message + the docs decision table, **not** a
hard rule (a `u4`+`u4` run is legitimately ambiguous; erroring on it would add
confusion, not remove it).

## Gotchas

- A catch-all `#[derive(BitEnum)]` mixes a tuple variant with discriminants;
  Rust forbids **explicit** discriminants there without `#[repr(..)]`. For
  contiguous-from-0 values the derive's auto-numbering works (drop the `= N`);
  only non-contiguous catch-all enums need `#[repr(u8)]` + explicit values.
- A no-`#[catch_all]` `BitEnum` whose variants don't cover its width is a **compile
  error** (the infallible `from_bits` codec/getter path would panic on an unknown
  discriminant). Add `#[catch_all] Other(uN)` to preserve unknowns (dual-use), or
  `#[bit_enum(uN, closed)]` to assert a closed set (then `from_bits` still panics on
  an out-of-set value; the checked `TryFrom` rejects it). A fully-covered enum
  (e.g. a 2-bit enum with all 4 variants) needs neither.
- Field widths must sum to `<= backing` bits (a generated `const` assert
  enforces it). A bitfield's `Bits::BITS` is the **declared total** width, not
  the backing width — that's what makes sub-byte nesting (`OpCode` = 5 bits in a
  `u8`) compose correctly.
- A fixed-length message implements `FixedBitLen` (its `BIT_LEN` sizes a nested
  region); a `count`-bearing (variable-length) message does **not**.

## Testing

**Six layers, by `mod` name** — every test lives in a named module so a level can be run on
its own (`cargo test <layer>` filters by the test's module path). **`unit` and `component`
live inline in the `src/` file they exercise** (they have a logical home next to the code);
`macro_`/`integration`/`e2e`/`property` live in `tests/` (no single src home). Put a new test
in the layer that matches its subject:

- **`unit`** — inline `#[cfg(test)] mod unit` in `src/*.rs`. Pure single-type logic, no macro
  expansion, no I/O (`UInt`, `Bits`/`Bitfield`, the `BitReader`/`BitWriter` cursor + `BitError`
  Display + the `Source`/`Sink` trait defaults via tiny in-test impls, `error`/`builder`).
- **`component`** — inline `#[cfg(test)] mod component` in `src/*.rs`, next to the adapter it
  exercises: `bitstream.rs` for the I/O ladder (`BufSource`/`SeekReader`/`StreamBitReader`/
  `BitBuf`/the `bytes` adapters), `net.rs` for the `mock`-driven `Message*` wrappers, `codec.rs`
  for the `tokio` `BinCodec`. (`#[bin]` works inside the crate's own test mods via
  `extern crate self as bnb`.)
- **`macro_`** — `tests/`, `mod macro_`. One generated surface over a slice (`#[bitfield]` /
  `BitEnum` / `bitflags` / `BitsBuilder` / bare derives / each `#[bin]` directive). `macro` is
  a keyword, hence `macro_`.
- **`integration`** — `tests/`, `mod integration`. Composed protocol shapes over slices
  (DNS/SMB in `protocol_shapes`, the DMR burst).
- **`e2e`** — `tests/`, `mod e2e`. Full transport sessions (`MessageStream`/`MessageDatagram`
  round-trips, `tokio` `Framed`/`UdpFramed`).
- **`property`** — `tests/`, `mod property`. `proptest` invariants (`fuzz_roundtrip`).

`tests/compile_fail.rs` (trybuild) is the separate negative/UI harness, outside the layers.

```bash
cargo test                                  # whole workspace (default features)
cargo test unit                             # one layer at a time (filters by mod name):
cargo test macro_                           #   unit · macro_ · component · integration · e2e · property
cargo test -p bitsandbytes --features bytes # + the bytes-crate I/O adapters
cargo test -p bitsandbytes --features mock  # + net socket helpers, mocks, the sealed-trait UI test
cargo test -p bitsandbytes --features tokio # + the async BinCodec (Framed/UdpFramed)
# Coverage (cargo-llvm-cov): the runtime crate `bnb/src` is 90–100% per file. The macro crate
# reads lower because its diagnostic arms are exercised only by trybuild (separate rustc
# processes llvm-cov can't instrument) — not a true gap; still test real generated paths.
cargo llvm-cov --all-features --ignore-filename-regex 'guide/|nostd-check/'
# no_std proof: build the detached smoke crate for a bare-metal target (std off).
# A host `--no-default-features` build still links std, so the cross target is the
# one that actually fails on a leak.
cargo build --manifest-path bnb/nostd-check/Cargo.toml --target thumbv7em-none-eabi
# MSRV floor (1.85): let-chains are unstable below 1.88 — DON'T use them; verify with:
cargo +1.85.0 check --workspace
```

- Inline tests in `src/*.rs`: `mod unit` (int ranges/conversions, `Bits` impls, the
  `BitReader`/`BitWriter` cursor + `BitError`, `error`/`builder`) and `mod component` (the I/O
  ladder in `bitstream.rs`, the `mock`-driven wrappers in `net.rs`, the `tokio` `BinCodec` in
  `codec.rs`).
- `tests/protocol_shapes.rs` — the **real** DNS `State` (0x1002), nested
  `OpCode`/`Flags` positions, catch-all preservation, exhaustive `Op`, SMB
  `SecurityMode` (LSB) / `Capabilities` (LE), manual ranges, and the `Bitfield`
  seam. Golden byte vectors.
- `tests/comprehensive.rs` — the full bitfield matrix: every backing (u8..u128),
  msb/lsb mirroring, all three width forms agreeing, masking/overflow,
  partial-width padding, 3-level nesting, byte-order, exhaustive/catch-all/
  contract-violation enums (incl. the documented panic for a non-exhaustive
  no-catch-all enum), and UInt boundaries + error `Display`.
- `tests/flags.rs` — `#[bitflags]`: consts, set algebra + operators, per-flag
  accessors, `iter`, retain vs truncate, nesting in a `#[bitfield]`.
- `tests/builder.rs` — `#[derive(BitsBuilder)]`: required-field errors, `default`
  / `default = expr`, the `#[bitfield]` intercept, and the plain-struct path.
- `tests/bin_*.rs` — the `#[bin]` surface, one concern per file: `bin_macro`
  (the fold), `bin_magic`, `bin_count` (+ `bin_count_adversarial`: hostile
  `count` — over-count → graceful EOF, `u32::MAX` → no pre-alloc, under-count
  → `TrailingBytes`), `bin_ctx`(+`_layer2`), `bin_map`,
  `bin_if`, `bin_calc_temp`, `bin_reserved`, `bin_ignore`, `bin_parse_with`,
  `bin_positioning`/`bin_restore_position`, `bin_validate`, `bin_byte_order`
  (+ `bin_order_matrix`: the message-level endian × bit-order 2×2),
  `bin_fold`. (The low-level I/O-ladder adapter tests — `BufSource`/`SeekReader`/`BitBuf`/the
  `bytes` adapters / `as_read`-`as_write` — moved to `src/bitstream.rs`'s `mod component`.)
- `tests/bitstream_*.rs` — the low-level derives/runtime: `bitstream_dmr`(+`_frame`)
  (the `108|48|108` DMR burst that motivated bit offsets), `bitstream_nested`,
  `bitstream_payload`, `bitstream_bitorder`, `bitstream_builder`, `bitstream_guard`
  (the right-tool-guard override). (`bitstream_source`/`_seek`/`_entry`/`_errors` moved to
  `src/bitstream.rs`'s `mod component`.)
- Transport `e2e` in `tests/`: `net.rs` (`--features net`, real sockets), `net_mock.rs`
  (`--features mock`, mock-stream session + loopback UDP), `codec_tokio.rs` (`--features tokio`,
  `Framed`/`UdpFramed` round-trips). The one-call `component` tests for these live inline in
  `src/net.rs` and `src/codec.rs`.
- `tests/compile_fail.rs` + `tests/ui/*` — trybuild snapshots proving `#[bin]` /
  `#[bitfield]` / derive misuse is rejected with a clear, well-spanned error
  (`bin_count_not_fixed`, `bin_ctx_needs_context`, `bin_forward_only_no_seek`,
  `bin_if_needs_option`, `bin_map_needs_inverse`, `bin_temp_needs_calc`,
  `bin_validate_needs_builder`, `bitfield_range_reversed`,
  `bitstream_byte_aligned`). `tests/ui_seal/*` (run under `--features mock`) proves the sealed
  `DatagramSocket` rejects a downstream impl. Regenerate with `TRYBUILD=overwrite`.

`#![deny(missing_docs)]` is on (both crates); the `uN` aliases are the one
allowed exception. Keep the public surface fully documented.

## Benchmarks

- `benches/bitfield_bench.rs` (criterion, `Criterion::default()`) measures `bnb`
  **against the crates it replaces** — `bitbybit`, `modular-bitfield-msb` (dev-deps,
  bench-only) — and a hand-written shift/mask baseline, on an identical DNS-shaped
  16-bit field. Result: `bnb` matches `bitbybit`, beats `modular-bitfield`, within
  noise of hand-written (pack ~870ps, unpack ~192ps).
- `benches/bitstream_bench.rs` — the `#[bin]`/derive codec throughput.

Run: `cargo bench -p bitsandbytes`.

## Examples

- `enums` — `#[derive(BitEnum)]` in depth: exhaustive, catch-all (the `num_enum`
  pattern), nesting, and checked-int error handling.
- `standalone` — building the IPv4 `0x45` byte from the field types directly.

## Scope notes

- Dual-use: `from_raw`/`from_be_bytes` and the parser never validate; `#[catch_all]`
  preserves unknown enum values; `#[bin]`'s `validate`/soundness is
  **construction-side only** (gates `build()`, leaves decode permissive). Keep
  that — never make a parser reject representable input. `validate = path` is also
  generated as re-runnable `validate()` / `is_valid()` methods (computed on demand — no
  stored "valid" flag, which would go stale on mutation). By convention `validate` checks
  **semantic** soundness, not `calc`/`reserved` fields (those are representational, normalized
  by `to_canonical_bytes`), so validity holds for the canonical form too; `to_canonical_bytes`
  itself stays a pure normalization (compose `validate()` before sending if you need the check).
- The `Bitfield` seam (`to_raw`/`from_raw` + the codec-agnostic trait) is the hook
  the `#[bin]` codec builds on; a value type stays codec-agnostic.
