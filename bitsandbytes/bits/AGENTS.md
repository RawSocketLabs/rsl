# bits / bits-macros

Workspace utility crates: a fast, integer-backed bit/byte field library with
binrw integration, meant to replace the external bitfield/int/enum helpers
(`modular-bitfield(-msb)`, `bitfield-struct`, `bitbybit`, `arbitrary-int`,
`num_enum`). **Phase 1** (this code): bitfields + ints + enums + a binrw bridge.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace
> root `AGENTS.md` also applies. **Not wired into any protocol crate yet** — by
> design. See `bits/DESIGN.md` for the full proposal, roadmap, and the
> build-vs-buy decision on a binrw replacement.

## Two-crate layout (required)

- `bits/` — runtime lib: `int` (`UInt<T, N>` + `u1..u127`), `field` (the `Bits`
  and `Bitfield` traits, `ByteOrder`/`BitOrder`), `error`, `builder`
  (`BuilderError`), and the crate root (re-exports + the `BitEnum` marker trait +
  `__private` for generated code).
- `bits-macros/` — proc-macro crate: `#[bitfield]`, `#[derive(BitEnum)]`,
  `#[bitflags]`, and `#[derive(BitsBuilder)]` (the `builder` module is shared by
  the standalone derive and the `#[bitfield]` intercept).

A proc-macro crate cannot also export runtime items, hence the split. Depend on
`bits`; it re-exports the macros.

## How the macro works (the load-bearing idea)

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
- binrw impls are emitted only when `cfg!(feature = "binrw")` (the macro
  inspects its *own* `binrw` feature, propagated from `bits`'s) and reference
  `::bits::__private::binrw::*`. A bitfield's binrw impl uses its **declared**
  byte order, ignoring the endian binrw passes in.
- A `BitEnum` gets binrw only when its width is a byte-aligned primitive
  (`u8`/`u16`/…); a sub-byte enum (`u4`) is meaningful only nested in a
  `#[bitfield]`.

## `#[bitflags]` and `#[derive(BitsBuilder)]`

- **`#[bitflags(uN)]`** takes `bool` fields (an attribute macro needs a valid
  struct, and a `bool` *is* a 1-bit flag); each auto-assigns a bit by position
  (LSB-first), or `#[flag(N)]` pins it. It generates UPPERCASE consts, set
  operators, `contains`/`iter`, per-flag `fin()`/`with_fin`/`set_fin`, and
  `Bits`/`Bitfield`/binrw — so a flag set nests in a `#[bitfield]`. `from_bits`
  **retains** unknown bits (dual-use); `from_bits_truncate` drops them.
- **`#[derive(BitsBuilder)]`** — required-by-default; `build() -> Result<_, BuilderError>`
  errors on the first unset field; `#[builder(default)]` / `#[builder(default = expr)]`
  opts a field out. **Intercept mechanism (load-bearing):** because `#[bitfield]`
  collapses the struct to one integer *before* derives run, a real derive can't
  see the logical fields — so `#[bitfield]` itself scans its derive list for
  `BitsBuilder` (`split_outer_attrs`), generates the builder from the fields, and
  strips the marker. A real `BitsBuilder` derive also exists for **plain**
  structs. So: put `#[bitfield(...)]` **above** `#[derive(BitsBuilder, ...)]`.

## Gotchas

- A catch-all `#[derive(BitEnum)]` mixes a tuple variant with discriminants;
  Rust forbids **explicit** discriminants there without `#[repr(..)]`. For
  contiguous-from-0 values the derive's auto-numbering works (drop the `= N`);
  only non-contiguous catch-all enums need `#[repr(u8)]` + explicit values.
- Field widths must sum to `<= backing` bits (a generated `const` assert
  enforces it). A bitfield's `Bits::BITS` is the **declared total** width, not
  the backing width — that's what makes sub-byte nesting (`OpCode` = 5 bits in a
  `u8`) compose correctly.

## Testing

```bash
RUSTC_WRAPPER= cargo test -p bits                      # default (with binrw)
RUSTC_WRAPPER= cargo test -p bits --no-default-features # standalone codec, no binrw
```

- `src/{int,field}.rs` unit tests — int ranges/conversions, `Bits` impls.
- `tests/protocol_shapes.rs` — reproduces the **real** DNS `State` (0x1002),
  nested `OpCode`/`Flags` positions, catch-all preservation, exhaustive `Op`,
  SMB `SecurityMode` (LSB) / `Capabilities` (LE), manual ranges, and the
  `Bitfield` seam. Golden byte vectors; runs with or without binrw.
- `tests/comprehensive.rs` — the full matrix: every backing (u8..u128), msb/lsb
  mirroring, all three width forms agreeing, masking/overflow, partial-width
  padding, 3-level nesting, byte-order, exhaustive/catch-all/contract-violation
  enums (incl. the documented panic for a non-exhaustive no-catch-all enum), and
  UInt boundaries + error `Display`. Codec-only (runs both feature configs).
- `tests/flags.rs` — `#[bitflags]`: consts, set algebra + operators, per-flag
  accessors, `iter`, retain vs truncate, and nesting in a `#[bitfield]`.
- `tests/builder.rs` — `#[derive(BitsBuilder)]`: required-field errors, `default`
  / `default = expr`, the `#[bitfield]` intercept, and the plain-struct path.
- `tests/binrw_integration.rs` (`#![cfg(feature = "binrw")]`) — the headline:
  bitfields/enums/flags in `#[binrw]` structs with no map glue, byte-aligned
  enums as binrw fields, and intrinsic (LE-in-BE) byte order.

`#![deny(missing_docs)]` is on (both crates); the `uN` aliases are the one
allowed exception. Keep the public surface fully documented.

## Benchmarks

`benches/bitfield_bench.rs` (criterion + pprof) measures `bits` **against the
crates it replaces** — `bitbybit`, `modular-bitfield-msb` (dev-deps, bench-only)
— and a hand-written shift/mask baseline, on an identical DNS-shaped 16-bit
field. Result: `bits` matches `bitbybit`, beats `modular-bitfield`, and is within
noise of hand-written (pack ~870ps, unpack ~192ps). Run: `cargo bench -p bits`;
flamegraphs with `-- --profile-time 5`.

## Examples

- `protocol_header` (binrw) — DNS-style collapsed header field.
- `ipv4_header` (binrw) — a **complete IPv4 header**: several bitfields + a
  byte-aligned enum + binrw `map` for addresses, producing a valid 20-byte
  packet header.
- `tcp_segment` (binrw) — **all three macros together**: `#[bitflags]` control
  flags inside a `#[bitfield]` + `#[derive(BitsBuilder)]` word, in a `#[binrw]`
  header.
- `enums` (codec-only) — `#[derive(BitEnum)]` in depth: exhaustive, catch-all
  (the `num_enum` pattern), nesting, and checked-int error handling.
- `standalone` (codec-only) — `bits` with `--no-default-features`, building the
  IPv4 `0x45` byte without binrw.

## Scope notes

- Phase 1 only: no `#[message]` derive (the binrw+builder+soundness folder) and
  no in-house codec yet — both are deferred in `DESIGN.md`. The `Bitfield` seam
  (`to_raw`/`from_raw` + the codec-agnostic trait) is the hook a future codec
  builds on.
- Dual-use: `from_raw`/`from_bytes` never validate; `#[catch_all]` preserves
  unknown enum values. Keep that — never make a parser reject representable
  input.
