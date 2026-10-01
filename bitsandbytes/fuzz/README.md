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
cargo +nightly fuzz run decode --fuzz-dir bitsandbytes/fuzz --target x86_64-unknown-linux-gnu
```

The tree pins stable via `rust-toolchain.toml`, so the explicit `+nightly` is required
locally. CI drops that file and defaults to nightly.

A crash drops a reproducer in `bitsandbytes/fuzz/artifacts/decode/`; replay it from
the repository root with:

```bash
cargo +nightly fuzz run decode bitsandbytes/fuzz/artifacts/decode/crash-<hash> --fuzz-dir bitsandbytes/fuzz --target x86_64-unknown-linux-gnu
```

## Targets

- **`decode`** — feeds the input to `decode_exact`/`peek`/`decode` across a spread of
  `#[bin]` shapes (byte-aligned header, sub-byte frame, catch-all enum, count-driven
  `Vec`, conditional `Option`, magic-prefixed), and asserts the fixed-length parsers
  are wire bijections. Mirrors the shapes in `bnb/tests/fuzz_roundtrip.rs`.
- **`stream_decode`** — sequences bounded push/rejection, pull/finite EOF, compaction,
  explicit growth, clear, and lossless handoff against an independent byte-accounting
  model; also fragments variable-width magic/fallback dispatch. It drives the actual borrowed
  sync and Tokio reader helpers over independently modeled length-prefixed records, varying
  transport chunks, scratch length and retained capacity. Async futures are dropped on
  `Pending` and recreated to prove completed reads remain in caller-owned storage. On error,
  raw handoff must recover every byte not consumed by a complete message. No runtime or socket
  is required. Buffer caps stay at most 128 bytes; reader-model input is capped at 512 bytes.
  Custom zero/unknown/huge hints, injected I/O errors, empty scratch and position-sensitive
  rebase are covered by deterministic component tests. Run
  `cargo +nightly fuzz run stream_decode --fuzz-dir bitsandbytes/fuzz
  --target x86_64-unknown-linux-gnu -- -runs=2000000` for the pre-commit run. CI uses a
  time-limited smoke run; that is not evidence that two million cases completed.

## Seed corpus

`corpus/decode/` holds one curated, valid encoding per shape (committed). The magic
prefix and `Option`-present paths are seeded because a blind fuzzer hits them slowly.
libFuzzer appends its own discoveries during a run; only the curated seeds are committed.
Use a temporary copy of the corpus for long local runs so discoveries do not clutter the
checkout; inspect and promote a small regression seed deliberately when it adds coverage.

The fuzz workspace's committed `Cargo.lock` pins this verification harness, not a
consumer's runtime dependencies. Refresh its path-package versions when preparing
the coordinated runtime/macros release and rerun both targets on that exact candidate.
CI validates the lock with `cargo metadata --locked` before cargo-fuzz builds.
