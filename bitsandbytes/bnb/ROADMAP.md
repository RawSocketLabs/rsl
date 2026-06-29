# `bnb` — status and capabilities

**Status: feature-complete, pre-1.0 (`0.x`); on the road to 1.0** — see
[Road to 1.0](#road-to-10). `bnb` is an owned, bit-aware binary codec — the field
types, macros, whole-message codec, and I/O ladder below are all built, tested, and
benchmarked. This file is the capability checklist; for the design rationale see
[`DESIGN.md`](DESIGN.md), for runnable walkthroughs the [`bnb::guide`] module and the
[`examples/`](examples/README.md) suite (indexed by feature), and for credit (binrw and the
bit/int/enum crates that inspired this one) [`ACKNOWLEDGMENTS.md`](ACKNOWLEDGMENTS.md).

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

- [x] Folds read + write codecs and the builder over one struct; generates the decode entry
      points — `decode(&mut Source)` (one cursor decode over the whole I/O ladder), `decode_all`/
      `decode_iter` (every message in a `&[u8]`, layout-baked + bit-aware), and `decode_exact`/`peek`
      (one-shot) — the encode entry points (`to_bytes` + the `encode(writer)` convenience, plus
      `BitEncode::bit_encode` for a `Sink`), and construction (`new(fields…)`, `builder()`).
- [x] **Verbatim vs canonical encode** — `to_bytes` is verbatim (exactly what's stored;
      byte-identical `decode → to_bytes`); `to_canonical_bytes` normalizes (`reserved` → spec,
      `calc` recomputed). Generated for a `reserved`/`calc` message, which also carries a
      wire-ignored `encode_mode` field (default `Verbatim`; the `std`-writer `encode(w)` follows
      it) and the in-memory helpers `to_canonical`/`canonical_diff`/`is_canonical`. The mode is
      excluded from eq/hash/`Debug`, so these types are builder/`new`/`decode`-constructed.
      **Struct-only** — a tagged-union enum encodes verbatim (no canonical/mode/`validate`/`new`).
- [x] **Struct options:** `big`/`little`, `bit_order = msb|lsb`, `magic = <expr>`
      (sub-byte allowed), `read_only`/`write_only`, `no_builder`, `forward_only`,
      `ctx(name: Ty, …)`, `validate = <path>`.
- [x] **Field directives:** `count`, `ctx { … }`, `temp` + `calc`, `if(…)`,
      `map`/`try_map` (+ inverse `bw(map)`), `parse_with`/`write_with`, `ignore`,
      `pad_*`/`align_*`, `restore_position`, `#[reserved]`/`#[reserved_with(…)]`,
      `#[try_str]` (a `Debug`-rendering hint: a byte buffer prints as a string when valid
      UTF-8, else hex bytes — never lossy; codec unaffected).
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
- [x] `BitBuf` — push/pull, bit-aware in-memory buffer: `push(&bytes)` as they arrive,
      `pull::<T>()` takes whole messages off the front (`None` until complete, reclaims as it
      goes). A `SeekSource`, so it also reads through plain `decode`; the pushable counterpart to
      `BufSource` (`no_std` + `alloc`).
- [x] `SeekReader<R: Read + Seek>` — large file / container.
- [x] `BytesReader`/`BytesWriter` — zero-copy `bytes`-crate framing (opt-in `bytes`
      feature).
- [x] `BinCodec<T>` — a `tokio_util::codec` `Decoder`/`Encoder` for any `#[bin]` message: drives
      both `Framed` (async TCP stream) and `UdpFramed` (async UDP datagrams, `(T, addr)`) — one
      codec, both transports (opt-in `tokio` feature; `examples/tokio_framed.rs`,
      `examples/tokio_udp.rs`).
- [x] `MessageStream` / `MessageDatagram` — ergonomic `std` socket helpers: whole-message
      `read_message`/`write_message` over a `Read + Write` stream (`TcpStream`; owns it, both
      directions, no `try_clone`) and `send_message`/`recv_message` over a **sealed**
      `DatagramSocket` (`UdpSocket` or `UnixDatagram`); both decode in the message's own layout
      (opt-in `net` feature; `examples/sockets.rs`, `examples/unix_stream.rs`).
- [x] `MockDatagramSocket` / `MockStream` — test-only in-memory transports (opt-in `mock` feature,
      for `[dev-dependencies]`) to unit-test `net` code without a real socket: scripted inbound,
      captured outbound, chunked delivery (driving the framing path `std::io::Cursor` can't), and
      error injection (`fail_after`/`fail_next_recv`). `mock` implies `net`; the `DatagramSocket`
      seal keeps these the only impls. `examples/mock_datagram.rs`, `examples/mock_stream.rs`.
- [x] Seeking enforced in the type system: a `restore_position` message's `decode`
      is bound on `SeekSource`, so a forward-only stream is a compile error.

## `no_std`

- [x] `no_std` + `alloc` behind a default-on **`std`** feature (Option A — buffer-at-a-
      time, not streaming). Without `std`: full macro surface, decode from `&[u8]`,
      encode to `Vec<u8>` (`to_bytes`/`to_canonical_bytes`, or `BitEncode::bit_encode` over a
      `Sink`). Verified by
      building `bnb/nostd-check` for `thumbv7em-none-eabi`.
- [x] `std` gates the `std::io` ladder (`StreamBitReader`/`BufSource`/`SeekReader`,
      `as_read`/`as_write`), `From<std::io::Error>`/`ErrorKind::Io`, and the
      `encode(writer)` extension trait (`EncodeExt`). `#[br(dbg)]` (a `tracing` event)
      is `std`-only.
- [ ] **Option B** (deferred) — an in-house `bnb::io` `Read`/`Write`/`Seek` abstraction
      to bring streaming I/O to `no_std` and unify the code path; revisit when an
      embedded byte-stream transport (TCP/serial) needs it.

## Cross-cutting

- [x] **Dual-use** — compliant defaults, permissive parsers (`#[catch_all]`, retained
      reserved/flag bits), construction-side `validate` (gates `build()`; never the parser) —
      also exposed as re-runnable `validate()`/`is_valid()` methods (computed, no stored flag)
      to re-check a value mutated since `build()` — and raw escape hatches.
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
- [x] Boundary stress: `u127`/`u128` incl. all-ones (`edge_cases.rs`), the endian ×
      bit-order matrix at both the bitfield layer (`comprehensive.rs`) and the message
      layer (`bin_order_matrix.rs` — the 2×2 compose-without-aliasing), sub-byte straddles
      (`bitstream_dmr`), and **attacker-controlled `count`** (`bin_count_adversarial.rs`:
      over-count → graceful `UnexpectedEof`, `u32::MAX` count → no pre-alloc, under-count
      → `TrailingBytes`, nested over-read keeps the innermost span).

### C. API freeze + SemVer tooling

- [ ] Deliberate public-API review: trait shapes (`BitDecode`/`BitEncode`/`Source`/
      `Sink`/`Bits`/`Bitfield`), the directive vocabulary, error types, and the
      `EncodeExt::encode(w)` / settable `encode_mode` / `EncodeMode` ergonomics — commit only to
      what you'll keep. Mark growth points `#[non_exhaustive]` (errors already are).
      **Scrutinize the encode/construct surface breadth:** a `reserved`/`calc` `#[bin]` struct
      exposes ~12 inherent methods here (`to_bytes`/`to_canonical_bytes` + `to_canonical`/
      `canonical_diff`/`is_canonical` + the `encode_mode` trio + `new`/`builder` + `validate`/
      `is_valid`). The inherent `encode_into`/`canonical_encode_into` sink writers were **cut** as
      redundant over the `BitEncode::bit_encode`/`canonical_bit_encode` trait methods (0 uses vs the
      trait's; sink-composition now uses the trait, symmetric with `encode(w)` needing `EncodeExt`).
      Still to weigh before the freeze: whether the full `encode_mode` trio (`set_`/`with_`/getter)
      and `canonical_diff` earn their slots, and — bigger — whether the carried-`encode_mode`
      mechanism pays for its complexity once dogfooding shows real streaming use.
- [x] `cargo-public-api` snapshot (`bnb/public-api.txt`, full surface via `--all-features`)
      + a CI `public-api` job that diffs it, pinned to `nightly-2026-06-17` +
      cargo-public-api `0.52` for reproducibility. Catches *unintended* surface drift; the
      committed snapshot is the reviewed baseline (regenerate deliberately on a real
      change). The proc-macro crate has no rustdoc-extractable surface — its macros are
      covered via the re-exports in the runtime-crate snapshot.
- [x] `cargo-semver-checks` in CI (`semver-checks` job, pinned to `0.48`) — checks the
      runtime crate against the latest release tag (`v{version}`, auto-advancing). **Run as
      informational** (`continue-on-error`): it surfaces SemVer breakage early as a heads-up
      but does not block, because release-plz already runs cargo-semver-checks and owns the
      version bump at release time (a break becomes a 0.x minor in the release PR). A
      blocking gate would force in-PR version bumps that fight that model. Complements
      `public-api` (which flags *any* surface change).
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
      git tags on merge; crates.io publishing deferred — see `docs/RELEASING.md`).
- [x] `CONTRIBUTING.md` (product-first, maintainer-decides model; issue-first for
      non-trivial work; inbound = outbound dual MIT/Apache; the local-checks + API-gate
      regen commands) and `SECURITY.md` (threat model, the dual-use "what is / isn't a
      vulnerability" scope, the security properties, private GitHub vulnerability reporting
      — now enabled). CI also runs **fuzz**, **public-api**, and **semver-checks** alongside
      fmt/clippy/test/no_std/deny/MSRV. **Miri** is the only outstanding gate
      (de-prioritized — see Section B).

### Open decisions to settle before 1.0 (each is a potential breaking change — do on `0.x`)

- [x] **`r` / `w` field-name collision** — *resolved*: the generated source/sink params are
      now `__bnb_r`/`__bnb_w`, so a user field named `r` or `w` no longer collides (the hard
      error is gone). Proof: `bin_macro.rs::fields_named_r_and_w_roundtrip`.
- [ ] **Option B (no_std streaming I/O)** — a 1.0 requirement, or explicit post-1.0
      (additive)? Document the boundary either way so it's an expectation, not a surprise.
- [x] **Encode model — `calc`/`reserved` handling, verbatim vs canonical** *(done — E1–E3 plus the
      runtime `EncodeMode`)*. `to_bytes` used to be an inconsistent hybrid (retained `reserved` but
      recomputed `calc`). **Shipped:**
      - **`to_bytes()` = verbatim** — emit exactly what's stored (retained `reserved` + stored
        non-`temp` `calc`). Matches the `to_bytes`/`as_bytes` ecosystem idiom, is dual-use-honest
        ("never silently rewrite what you gave me"), and restores a byte-identical `decode → to_bytes`
        round-trip as the default. (`temp`+`calc` fields are never stored, so they always recompute.)
      - **`to_canonical_bytes()` = canonical** — normalize `reserved` to its spec value, recompute
        `calc`; always a valid, spec-compliant message. Generated whenever a struct has a `reserved`
        or non-`temp` `calc` field, alongside the in-memory helpers `to_canonical(self) -> Self` /
        `canonical_diff` / `is_canonical`.
      - **Mode carried on the value:** a message with a `reserved`/`calc` field gains a settable,
        wire-ignored `encode_mode` field (`EncodeMode { Verbatim, Canonical }`, default `Verbatim`)
        — set via the builder's `.encode_mode(…)`, `set_encode_mode`/`with_encode_mode`; read via
        `encode_mode()`. The std-writer `encode(w)` follows it (no `mode` parameter); `to_bytes`/
        `to_canonical_bytes` stay explicit. The mode is **excluded from `PartialEq`/`Eq`/`Hash`/
        `Debug`** (a render preference, not data; `#[bin]` intercepts those derives), so these types
        are **builder/`decode`-constructed** (the field can't appear in a literal). The canonical
        path is a **defaulted method on `BitEncode`** (`canonical_bit_encode`, default = `bit_encode`),
        so there is **no separate `CanonicalEncode`/`CanonicalEncodeExt` trait**. Decided against
        `read`/`write` naming (collides with the `Source::read`/`Sink::write` cursor layer), against a
        `bool`/Vec-dispatcher, and (after first shipping `encode(w, mode)`) against a call-time mode
        parameter in favor of the carried field.
      - **No `encode_mixed`** — per-field selection is covered by the value-level `#[brw(ignore)]` idiom.
      - **No `decode_canonical`** — one permissive `decode()` (verbatim) stays; normalize-on-read loses
        dual-use info and validate-on-read would reject input (both anti-dual-use).
- [x] **Bitfield `Debug`** *(done)* — `#[bitfield]` intercepts a `#[derive(Debug)]` and emits a
      custom impl decomposing the **logical** fields (`version: u4(4), ihl: u4(5)`) instead of
      the opaque backing int (`{ value: 69 }`); bitfields nested in `#[bin]` structs inherit it.
- [x] **Canonical helpers** *(done)* — generated alongside `to_canonical_bytes` (when a
      message has a `reserved` or non-`temp` `calc` field): `to_canonical(self) -> Self` (the
      in-memory canonical form — reserved → spec, `calc` → recomputed), `canonical_diff(&self)
      -> Vec<&'static str>` (names of fields differing from canonical), and `is_canonical(&self)
      -> bool`. `Debug` stays the stored state. Closes the encode-model arc (E1–E3).
- [ ] **`#[default]` for `BitEnum` + struct field defaults** (all additive). (1) a `#[default]`
      variant marker so `Enum::default()` is well-defined — std `#[derive(Default)]` already
      covers *unit-only* enums, so bnb only needs its own for the `catch_all` case; (2)
      `#[default(<value>)]` on the `catch_all` variant (e.g. `#[default(0)] Other(u8)`) — beyond
      std, since the default carries a value; (3) per-field `#[default(<expr>)]` composing into a
      real `Default` impl for `#[bin]`/`#[bitfield]` structs (today only the builder-only
      `#[builder(default = expr)]` exists, and bitfields get an all-zero `Default`).
- [ ] **Encode-model parity for tagged-union enums** — the canonical/`encode_mode`/`validate`/
      `new` surface is currently **struct-only**; a `#[bin]` enum encodes verbatim even if a variant
      has a `reserved`/`calc` field. Decide before 1.0 whether to (a) bring parity (the enum
      delegates to its selected variant's canonical form / validity), or (b) keep it struct-only and
      document the boundary as intentional (done in the guide/DESIGN). Additive either way.
- [ ] **Scope line** — is `serde` interop / an `async` codec in scope for 1.0, or
      explicitly out? Decide now so 1.0's surface is intentional.
