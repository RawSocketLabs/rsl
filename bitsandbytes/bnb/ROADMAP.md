# `bnb` — status and capabilities

**Status: feature-complete (`0.1.0`); on the road to 1.0** — see
[Road to 1.0](#road-to-10). `bnb` is an owned, bit-aware binary codec — the field
types, macros, whole-message codec, and I/O ladder below are all built, tested, and
benchmarked. This file is the capability checklist; for the design rationale see
[`DESIGN.md`](DESIGN.md), for runnable walkthroughs the [`bnb::guide`] module, and for
credit (binrw and the bit/int/enum crates that inspired this one)
[`ACKNOWLEDGMENTS.md`](ACKNOWLEDGMENTS.md).

[`bnb::guide`]: https://docs.rs/bnb/latest/bnb/guide/

## Field types & macros

- [x] **`u1`..`u127`** (`UInt<T, N>`) — range-checked arbitrary-width unsigned
      integers; `new`/`try_new`/`from_raw`, `From`/`TryFrom`, `MIN`/`MAX`.
- [x] **`#[bitfield]`** — integer-backed packing with independent `bits = msb|lsb` and
      `bytes = be|le`; inferred / `#[bits(N)]` / `#[bits(A..=B)]` width forms; getters,
      `with_*`/`set_*`, `*_bytes`; nests in other bitfields and in `#[bin]`.
- [x] **`#[derive(BitEnum)]`** — enum ⇄ integer at a chosen width; `#[catch_all]`
      (lossless, dual-use) or `closed` (asserted closed set); a non-exhaustive enum
      with neither is a compile error; `num_enum`-parity `From`/`TryFrom` for
      byte-aligned widths.
- [x] **`#[bitflags]`** — single-bit flag sets with set algebra, per-flag accessors,
      `iter`, retain-vs-truncate.
- [x] **`#[derive(BitsBuilder)]`** — required-by-default builder; `build()` names the
      first unset field; `#[builder(default)]` / `#[builder(default = expr)]`.

## The `#[bin]` whole-message codec

- [x] Folds read + write codecs and the builder over one struct; generates
      `decode`/`peek`/`decode_exact`/`decode_from`, `encode`/`to_bytes`/`encode_into`,
      `builder()`.
- [x] **Struct options:** `big`/`little`, `bit_order = msb|lsb`, `magic = <expr>`
      (sub-byte allowed), `read_only`/`write_only`, `no_builder`, `forward_only`,
      `ctx(name: Ty, …)`, `validate = <path>`.
- [x] **Field directives:** `count`, `ctx { … }`, `temp` + `calc`, `if(…)`,
      `map`/`try_map` (+ inverse `bw(map)`), `parse_with`/`write_with`, `ignore`,
      `pad_*`/`align_*`, `restore_position`, `#[reserved]`/`#[reserved_with(…)]`.
- [x] Lowers to `#[derive(BitDecode, BitEncode, BitsBuilder)]`; the bare derives carry
      the all-byte-aligned right-tool guard (escape hatch
      `#[bit_stream(allow_byte_aligned)]`).
- [x] **Tagged-union enums** (`#[bin]` on an enum) — dispatch by per-variant `magic` (a
      wire constant: byte string or width-suffixed int), by a read-only `tag` selector
      drawn from `ctx` (never on the wire), an enum-level `magic` prefix, or a hybrid of
      the two; `#[catch_all]` preserves an unknown discriminant (else a closed set is a
      decode error); variable-width / typed-fallback magics peek+seek; `magic()`/`tag()`
      accessors plus `decode_as_<variant>`/`peek_variant`/`decode_tagged` helpers. See
      [`bnb::guide::dispatch`].
- [x] **`ctx` is decode-only** — `decode_with` + a generated `…Ctx` (built positionally
      with `…Ctx::new`) carry parse context; encode stays a plain `to_bytes` unless the
      *write* side reads a ctx param (a keyed `bw(map)`/`calc`/`write_with`), then it gets
      `to_bytes_with`/`encode_with`. A variant `Vec` field can forward per-element `ctx`.
      `DecodeWith<A>`/`EncodeWith<A>` are the polymorphic companions — one bound spans
      context-free and context-taking messages.

## I/O ladder

- [x] `BitReader`/`BitWriter` — bit cursors over a byte buffer (seek is free cursor
      math; no `Seek` trait).
- [x] `StreamBitReader<R: Read>` — forward-only streaming; `Incomplete` ("read more")
      signal.
- [x] `BufSource<R: Read>` — bounded retain-and-seek socket adapter.
- [x] `SeekReader<R: Read + Seek>` — large file / container.
- [x] `BytesReader`/`BytesWriter` — zero-copy `bytes`-crate framing (opt-in `bytes`
      feature).
- [x] Seeking enforced in the type system: a `restore_position` message's `decode_from`
      is bound on `SeekSource`, so a forward-only stream is a compile error.

## `no_std`

- [x] `no_std` + `alloc` behind a default-on **`std`** feature (Option A — buffer-at-a-
      time, not streaming). Without `std`: full macro surface, decode from `&[u8]`,
      encode to `Vec<u8>` (`to_bytes`/`to_spec_bytes`/`encode_into`). Verified by
      building `bnb/nostd-check` for `thumbv7em-none-eabi`.
- [x] `std` gates the `std::io` ladder (`StreamBitReader`/`BufSource`/`SeekReader`,
      `as_read`/`as_write`), `From<std::io::Error>`/`ErrorKind::Io`, and the
      `encode(writer)`/`spec_encode(writer)` extension traits (`EncodeExt`/`SpecEncodeExt`).
      `#[br(dbg)]` (a `tracing` event) is `std`-only.
- [ ] **Option B** (deferred) — an in-house `bnb::io` `Read`/`Write`/`Seek` abstraction
      to bring streaming I/O to `no_std` and unify the code path; revisit when an
      embedded byte-stream transport (TCP/serial) needs it.

## Cross-cutting

- [x] **Dual-use** — compliant defaults, permissive parsers (`#[catch_all]`, retained
      reserved/flag bits), construction-side-only `validate`, raw escape hatches.
- [x] **Position-aware errors** — `BitError` carries the bit offset + field; the codec
      `Error`/`UnknownDiscriminant`/`BuilderError` cover construction.
- [x] **Performance** — shift/mask bitfields (matches `bitbybit`, within noise of
      hand-written); byte-aligned fast path in the stream codec; `#[inline]` hot path.
- [x] **Zero `unsafe`** — `unsafe_code = "forbid"` (workspace lints) across both crates
      and every target; the macros emit no `unsafe`, so a consumer can `#![forbid(unsafe_code)]`
      and still use `bnb`. A bit-level codec with no `unsafe` is a deliberate selling point.

## Testing

- [x] Per-directive success tests + the comprehensive bitfield matrix + real protocol
      shapes (DNS/SMB/DMR) + golden byte vectors.
- [x] Property-based round-trips and a robustness suite ("decode of arbitrary bytes
      never panics") across many shapes.
- [x] All runtime error kinds asserted; trybuild compile-fail snapshots for the macro
      misuse surface.

## Road to 1.0

`bnb` is *feature-complete* but not yet *1.0-stable*. **1.0 is a SemVer promise — no
breaking changes until 2.0** — so the gate is real-world API validation, not feature
count; it stays on `0.x` (where breaking changes are cheap) until the surface has
survived real use. Suggested order: **A** and **B** run in parallel and drive **C**
(the API shakes out from real use); **D/E/F** are polish; then soak on a late `0.x`
(e.g. `0.9`) in a real tool for a release or two and tag **`1.0.0`** once a cycle
passes with no breaking change needed.

### A. Live testing / dogfooding — load-bearing (`bnb` has no real consumer yet)

- [ ] Port 2–3 real protocols onto `bnb` — DNS is the flagship (rich bitfields, golden
      vectors, refcheck RFC tracking); `nbt`/`smb` come off `modular-bitfield`;
      `icmp`/`tftp` exercise the `#[bin]` message codec. The `asyio/protocols` crates
      (the stack `bnb` was built to replace) are the proving ground.
- [ ] Each ported crate passes its **existing** suite **byte-identically** (golden vectors).
- [ ] Decode **real captured traffic** (pcaps) and interop against a **live peer** — the
      dual-use story: emit to a real server / fuzz a real client.

### B. Correctness hardening — it parses untrusted bytes

- [x] A `cargo-fuzz` target on the decode path (`fuzz/fuzz_targets/decode.rs` — promotes
      the "decode of arbitrary bytes never panics" proptest, plus the fixed-parser
      bijection assert; curated seed corpus). Wired into CI (the `fuzz` job: build +
      time-boxed smoke run under ASan/UBSan). Remaining: submit to **OSS-Fuzz**.
- [x] **Zero `unsafe`** — enforced crate-wide by `unsafe_code = "forbid"` in the
      workspace lints (a guarantee an `#[allow]` can't reopen, unlike `deny`). The macros
      never *emit* `unsafe` either, so the guarantee carries into consumer code.
- [ ] **Miri** over the test suite — lower priority now that `unsafe` is forbidden (Miri
      mainly hunts UB reachable through `unsafe`); keep as a backstop for the codec's
      slice/offset arithmetic, but it no longer gates 1.0.
- [ ] Differential correctness vs `binrw`/`modular-bitfield` on shared shapes (the bench
      targets already exist).
- [ ] Boundary stress: `u127`, the full endian × bit-order matrix, sub-byte straddles,
      attacker-controlled `count` (the push-based `Vec` guard).

### C. API freeze + SemVer tooling

- [ ] Deliberate public-API review: trait shapes (`BitDecode`/`BitEncode`/`Source`/
      `Sink`/`Bits`/`Bitfield`), the directive vocabulary, error types, and the
      `EncodeExt`/`SpecEncodeExt` ergonomics — commit only to what you'll keep. Mark
      growth points `#[non_exhaustive]` (errors already are).
- [x] `cargo-public-api` snapshot (`bnb/public-api.txt`, full surface via `--all-features`)
      + a CI `public-api` job that diffs it, pinned to `nightly-2026-06-17` +
      cargo-public-api `0.52` for reproducibility. Catches *unintended* surface drift; the
      committed snapshot is the reviewed baseline (regenerate deliberately on a real
      change). The proc-macro crate has no rustdoc-extractable surface — its macros are
      covered via the re-exports in the runtime-crate snapshot.
- [x] `cargo-semver-checks` in CI (`semver-checks` job, pinned to `0.48`) — checks the
      runtime crate against the latest release tag (`v{version}`, auto-advancing) and
      **blocks** on SemVer-breaking changes until the version is bumped, so breakage is
      deliberate. Complements `public-api` (which flags *any* surface change).
- [ ] Lock the MSRV (1.85) and feature-flag set as part of the contract.

### D. Docs & migration

- [ ] Migration guide (`modular-bitfield`/`binrw`/`num_enum` → `bnb`) and a
      `CHANGELOG.md` (the `guide` module, `DESIGN.md`, docs.rs + Pages are already done).

### E. Performance baseline

- [ ] Throughput on **real whole-messages** (not just a 16-bit field); a CI
      perf-regression gate; a macro compile-time / codegen-bloat sanity check.

### F. Release hygiene

- [x] Conventional-Commit enforcement (commitlint CI) + Conventional-Commit-driven
      release automation (`release-plz`: per-crate `CHANGELOG.md` + SemVer-bump PRs,
      git tags on merge; crates.io publishing deferred — see `RELEASING.md`).
- [ ] `CONTRIBUTING.md` / `SECURITY.md`; CI jobs for fuzz (✓ done) + Miri +
      semver-checks alongside the existing fmt/clippy/test/no_std/deny/MSRV set.

### Open decisions to settle before 1.0 (each is a potential breaking change — do on `0.x`)

- [x] **`r` / `w` field-name collision** — *resolved*: the generated source/sink params are
      now `__bnb_r`/`__bnb_w`, so a user field named `r` or `w` no longer collides (the hard
      error is gone). Proof: `bin_macro.rs::fields_named_r_and_w_roundtrip`.
- [ ] **Option B (no_std streaming I/O)** — a 1.0 requirement, or explicit post-1.0
      (additive)? Document the boundary either way so it's an expectation, not a surprise.
- [ ] **`encode(writer)` ergonomics** — keep the `use bnb::prelude::*` extension-trait
      form, or reconsider while it's still cheap?
- [ ] **Scope line** — is `serde` interop / an `async` codec in scope for 1.0, or
      explicitly out? Decide now so 1.0's surface is intentional.
