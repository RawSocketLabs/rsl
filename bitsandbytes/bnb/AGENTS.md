# bnb / bnb-macros

> **History:** the crates were `bits`/`bits-macros` (renamed Phase 5) and the
> codec began as a binrw bridge. **binrw has since been fully removed** — `bnb`
> is now a self-contained, owned codec. For the full phase plan (all ✓) see
> **`ROADMAP.md`**; `bnb/DESIGN.md` has the proposal and build-vs-buy decision.

Workspace utility crates: a fast, **owned bit-aware** binary codec, integer-backed,
that replaces the external bitfield/int/enum/codec stack (`modular-bitfield(-msb)`,
`bitfield-struct`, `bitbybit`, `arbitrary-int`, `num_enum`, and our former use of
`binrw`). The unified `#[bin]` attribute is the whole-message front-end
(magic/count/ctx/map/if/calc·temp/reserved/positioning/validate + a
`Source`/`SeekSource`/`BufSource`/`SeekReader` I/O ladder, opt-in `bytes`). **There
is no binrw dependency or feature** — the codec is entirely in-house.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace
> root `AGENTS.md` also applies. **Not wired into any protocol crate yet** — by
> design.

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

A proc-macro crate cannot also export runtime items, hence the split. Depend on
`bnb`; it re-exports the macros.

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

`#[bin]` is the owned successor to our former binrw usage: one attribute that
folds the codec (`BitDecode`/`BitEncode`) and the required-by-default builder over
a struct, generating `decode`/`peek`/`decode_exact`, `encode`/`to_bytes`, and
`Foo::builder()`. It reads/writes fields at **arbitrary bit offsets**, so the same
attribute handles byte-aligned headers and sub-byte frames alike.

- **Lowering.** `#[bin]` lowers to `#[derive(BitDecode, BitEncode, BitsBuilder)]`
  + `#[bit_stream(...)]`; the field-directive logic lives in those derives, which
  stay usable directly. `#[bin]` is a thin, zero-duplication front-end over them.
- **Struct-level options:** `read_only` / `write_only` (directional codecs),
  `no_builder`, `bit_order = msb|lsb`, `bytes = be|le` (`big`/`little`),
  `allow_byte_aligned`.
- **Field directives** (the inherited grammar): `#[br]`/`#[bw]`/`#[brw]` with
  `magic`, `count`, `ctx`/`args`, `map`/`try_map`, `if`, `calc`/`temp`,
  `reserved`/`reserved_with`, `parse_with`/`write_with`, `pad_before`/`pad_after`/
  `align_*`/`seek`/`restore_position`, and `assert`/`validate`. Positioning amounts
  use the `prelude` typed helpers (`4.bits()`, `3.bytes()`).
- **I/O ladder** (`bnb::bitstream`): `Source`/`Sink` (the bit cursors), the
  `SeekSource` marker for in-memory buffers, `BufSource<R: Read>` (bounded
  retain-and-seek over a forward-only reader), `SeekReader<R: Read + Seek>`, and —
  under the opt-in **`bytes`** feature — `BytesReader`/`BytesWriter` for async
  framing. Seeking is free cursor math; there is no uniform `Seek` requirement.

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

```bash
RUSTC_WRAPPER= cargo test -p bnb                 # core codec (default features)
RUSTC_WRAPPER= cargo test -p bnb --features bytes # + the bytes-crate I/O adapters
```

- `src/{int,field}.rs` unit tests — int ranges/conversions, `Bits` impls.
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
  (the fold), `bin_magic`, `bin_count`, `bin_ctx`(+`_layer2`), `bin_map`,
  `bin_if`, `bin_calc_temp`, `bin_reserved`, `bin_ignore`, `bin_parse_with`,
  `bin_positioning`/`bin_restore_position`, `bin_validate`, `bin_byte_order`,
  `bin_fold`, and the I/O ladder (`bin_buf_source`, `bin_seek_reader`,
  `bin_bytes` — the last under `--features bytes`).
- `tests/bitstream_*.rs` — the low-level derives/runtime: `bitstream_dmr`(+`_frame`)
  (the `108|48|108` DMR burst that motivated bit offsets), `bitstream_nested`,
  `bitstream_payload`, `bitstream_bitorder`, `bitstream_source`, `bitstream_seek`,
  `bitstream_errors`, `bitstream_builder`, `bitstream_entry`, `bitstream_guard`
  (the right-tool-guard override).
- `tests/compile_fail.rs` + `tests/ui/*` — trybuild snapshots proving `#[bin]` /
  `#[bitfield]` / derive misuse is rejected with a clear, well-spanned error
  (`bin_count_not_fixed`, `bin_ctx_needs_context`, `bin_forward_only_no_seek`,
  `bin_if_needs_option`, `bin_map_needs_inverse`, `bin_temp_needs_calc`,
  `bin_validate_needs_builder`, `bitfield_range_reversed`,
  `bitstream_byte_aligned`). Regenerate with `TRYBUILD=overwrite`.

`#![deny(missing_docs)]` is on (both crates); the `uN` aliases are the one
allowed exception. Keep the public surface fully documented.

## Benchmarks

- `benches/bitfield_bench.rs` (criterion, shared `testutil::bench`) measures `bnb`
  **against the crates it replaces** — `bitbybit`, `modular-bitfield-msb` (dev-deps,
  bench-only) — and a hand-written shift/mask baseline, on an identical DNS-shaped
  16-bit field. Result: `bnb` matches `bitbybit`, beats `modular-bitfield`, within
  noise of hand-written (pack ~870ps, unpack ~192ps).
- `benches/bitstream_bench.rs` — the `#[bin]`/derive codec throughput.

Run: `cargo bench -p bnb`. Flamegraphs are opt-in via `testutil/profiling`.

## Examples

- `enums` — `#[derive(BitEnum)]` in depth: exhaustive, catch-all (the `num_enum`
  pattern), nesting, and checked-int error handling.
- `standalone` — building the IPv4 `0x45` byte from the field types directly.

## Scope notes

- Dual-use: `from_raw`/`from_bytes` and the parser never validate; `#[catch_all]`
  preserves unknown enum values; `#[bin]`'s `validate`/soundness is
  **construction-side only** (gates `build()`, leaves decode permissive). Keep
  that — never make a parser reject representable input.
- The `Bitfield` seam (`to_raw`/`from_raw` + the codec-agnostic trait) is the hook
  the `#[bin]` codec builds on; a value type stays codec-agnostic.
