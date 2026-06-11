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
  and `Bitfield` traits, `ByteOrder`/`BitOrder`), `error`, and the crate root
  (re-exports + the `BitEnum` marker trait + `__private` for generated code).
- `bits-macros/` — proc-macro crate: `#[bitfield]` and `#[derive(BitEnum)]`.

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
- `tests/binrw_integration.rs` (`#![cfg(feature = "binrw")]`) — the headline:
  bitfields/enums in `#[binrw]` structs with no map glue, byte-aligned enums as
  binrw fields, and intrinsic (LE-in-BE) byte order.

Examples: `cargo run -p bits --example protocol_header` (binrw, DNS-style);
`cargo run -p bits --example standalone [--no-default-features]` (codec-only,
builds the IPv4 `0x45` byte).

## Scope notes

- Phase 1 only: no `#[message]` derive (the binrw+builder+soundness folder) and
  no in-house codec yet — both are deferred in `DESIGN.md`. The `Bitfield` seam
  (`to_raw`/`from_raw` + the codec-agnostic trait) is the hook a future codec
  builds on.
- Dual-use: `from_raw`/`from_bytes` never validate; `#[catch_all]` preserves
  unknown enum values. Keep that — never make a parser reject representable
  input.
