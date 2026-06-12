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
  `#[bitflags]`, `#[derive(BitsBuilder)]` (the `builder` module is shared by the
  standalone derive and the `#[bitfield]` intercept), and `#[wire]` (the
  whole-header folder, gated on the `binrw` feature).

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

## `#[wire]` (the whole-header folder)

Folds binrw + builder + collapsed bit-groups + derived fields + soundness into
one attribute. **It is sugar, not a new codec:** `wire::expand` rewrites the
struct into a `#[::binrw::binrw]` struct (so the *entire* binrw attribute surface
— `magic`, `count`, `args`, `map`, `parse_with`, `if`, `pre_assert`, … — stays
usable as an escape hatch), emits a private `#[::bits::bitfield]` per group, and
calls `builder::generate` for the builder. Lives in `bits-macros/src/wire.rs`;
re-exported from `bits` only under the `binrw` feature; the **dependent crate
needs `binrw` as a direct dep** (generated code names `::binrw`).

Load-bearing details:

- **Groups lower via the binrw temp/calc pair.** `group(a, b => uN)` (struct-level,
  by name) inserts a `#[br(temp)] #[bw(calc = Grp::new().with_a(self.a)…)]` packed
  word and turns each member into `#[br(calc = grp.a())] #[bw(ignore)]`. Generating
  the read/write halves *together* sidesteps binrw #47 (a read-`temp` field is not
  auto-`calc` on write) — the two directions can't drift. The temp word is removed
  from the struct, so there's no DNS-style 2-byte bloat. Verified the pattern by
  hand before building the macro.
- **Group validation is the user's safety requirement:** named members must be
  **consecutive and in declared order**; the macro errors (well-spanned, at the
  offending ident) otherwise, so a moved field is a compile error. Members must
  also **fill the backing exactly** — a generated const-eval assert (`Σ member
  BITS == backing BITS`, wrapped in `#[allow(clippy::identity_op)]`) rejects an
  under-/over-filled group, since the bitfield would otherwise silently
  right-align a short group and pad the high bits (a latent wire bug). Generic
  params/lifetimes are rejected (not threaded into the group bitfields/builder).
- `#[update(expr)]` → `#[br(temp)] #[bw(calc = expr)]` (not stored, not in the
  builder; expr references fields via `self.`). `#[builder_only(default = e)]` →
  `#[br(calc = e)] #[bw(ignore)]` and the builder default becomes `e` (wire both:
  the read-side calc *and* the builder default must use the same expr — a fixed
  bug was setting only the former).
- **Soundness is construction-side only, by design.** `validate = path` auto-adds
  a `pub check_soundness: bool` (`#[br(calc = true)] #[bw(ignore)]`, default true)
  and a `validate(&self)` method; `build()` calls it via a `post_build` hook on
  `builder::generate`. **The parser stays permissive** — auto-validating `BinRead`
  would violate the dual-use rule (never reject representable input). The
  validator returns any `Display` error; it flows out as `BuilderError::Invalid`
  (the enum gained that case).

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
- `tests/wire.rs` (`#![cfg(feature = "binrw")]`) — `#[wire]`: group packing
  + round-trip, derived `#[update]` counts + count-driven `Vec`s, required-field
  errors, soundness dual-use (gates build, permissive parser, opt-in `validate()`,
  `check_soundness(false)` escape hatch), `#[builder_only]` off-wire, multi-group
  + little-endian, `no_builder`, binrw `map`/`magic` passthrough, and a capstone
  using every feature in one header.
- `tests/compile_fail.rs` + `tests/ui/*` — trybuild snapshots proving `#[wire]`
  misuse (non-adjacent / out-of-order / unknown / duplicate group members, marker
  conflicts, tuple struct, under-filled group, generic struct) is rejected with a
  clear, well-spanned error. Regenerate with `TRYBUILD=overwrite`.
- `tests/wire_proptest.rs` (proptest) — property round-trips: `encode∘decode = id`
  over random field values, and `decode∘encode = id` over random bytes (parser is
  total; the group word is a bijection).
- `tests/wire_golden.rs` — real DNS header byte-vectors (RFC 1035 §4.1.1, flags
  word as an 8-member group): query / NXDOMAIN response / opcode-high-bits.
- `tests/wire_stress.rs` — edge matrix: LE multi-byte group, back-to-back groups,
  nested `#[bitfield]` member, `builder_only` w/o default, user-declared
  `check_soundness`, `validate` + `no_builder`, custom `Display` validator error,
  `#[wire]`-in-`#[wire]`, group-type-name disambiguation.
- `tests/binrw_integration.rs` (`#![cfg(feature = "binrw")]`) — the headline:
  bitfields/enums/flags in `#[binrw]` structs with no map glue, byte-aligned
  enums as binrw fields, and intrinsic (LE-in-BE) byte order.

`#![deny(missing_docs)]` is on (both crates); the `uN` aliases are the one
allowed exception. Keep the public surface fully documented.

## Benchmarks

`benches/bitfield_bench.rs` (criterion (shared `testutil::bench`)) measures `bits` **against the
crates it replaces** — `bitbybit`, `modular-bitfield-msb` (dev-deps, bench-only)
— and a hand-written shift/mask baseline, on an identical DNS-shaped 16-bit
field. Result: `bits` matches `bitbybit`, beats `modular-bitfield`, and is within
noise of hand-written (pack ~870ps, unpack ~192ps). Run: `cargo bench -p bits`;

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
- `wire_header` (binrw) — a **DNS-style header in one `#[wire]`**: a
  bit-group, derived counts, soundness, and the builder, with the before/after
  framing in the file header.

## Scope notes

- `#[wire]` (the binrw + builder + bit-group + soundness folder) is built
  (DESIGN §9). It **wraps** binrw rather than replacing it — an in-house codec
  (DESIGN §4's option 2b) is still deferred; `#[wire]` does not force it. The
  `Bitfield` seam (`to_raw`/`from_raw` + the codec-agnostic trait) remains the
  hook a future codec would build on.
- Dual-use: `from_raw`/`from_bytes` never validate; `#[catch_all]` preserves
  unknown enum values. Keep that — never make a parser reject representable
  input.
