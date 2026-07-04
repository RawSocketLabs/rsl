# `bnb` fuzzing

Coverage-guided fuzzing of the **decode path** — the dual-use safety contract that a
parser fed hostile or garbage bytes returns `Ok`/`Err` but **never panics, reads out
of bounds, or loops unboundedly**. This promotes the `decode_arbitrary_bytes_never_panics`
proptest in [`bnb/tests/fuzz_roundtrip.rs`](../bnb/tests/fuzz_roundtrip.rs) to a
continuous, sanitizer-backed (ASan/UBSan) fuzzer.

This is a **separate workspace** (note the empty `[workspace]` table in `Cargo.toml`)
so the unstable `libfuzzer-sys` toolchain never touches the parent workspace's stable
fmt/clippy/test/no_std/deny/MSRV jobs.

## Run it

Needs nightly (`libFuzzer` + sanitizers) and `cargo-fuzz`:

```bash
cargo install cargo-fuzz            # once
cargo +nightly fuzz run decode      # fuzz until a crash or Ctrl-C
```

The tree pins stable via `rust-toolchain.toml`, so the explicit `+nightly` is required
locally. CI drops that file and defaults to nightly.

A crash drops a reproducer in `fuzz/artifacts/decode/`; replay it with:

```bash
cargo +nightly fuzz run decode fuzz/artifacts/decode/crash-<hash>
```

## Targets

- **`decode`** — feeds the input to `decode_exact`/`peek`/`decode` across a spread of
  `#[bin]` shapes (byte-aligned header, sub-byte frame, catch-all enum, count-driven
  `Vec`, conditional `Option`, magic-prefixed), and asserts the fixed-length parsers
  are wire bijections. Mirrors the shapes in `bnb/tests/fuzz_roundtrip.rs`.

## Seed corpus

`corpus/decode/` holds one curated, valid encoding per shape (committed). The magic
prefix and `Option`-present paths are seeded because a blind fuzzer hits them slowly.
libFuzzer appends its own discoveries during a run; only the curated seeds are committed
(see the repo `.gitignore`).
