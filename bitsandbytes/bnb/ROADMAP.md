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
      `bytes = big|le`; inferred / `#[bits(N)]` / `#[bits(A..=B)]` width forms; getters,
      `with_*`/`set_*`, order-respecting `to_bytes`/`from_bytes` (the declared `bytes`) plus the
      endianness-explicit `to_be_bytes`/`to_le_bytes` override; nests in other bitfields and in `#[bin]`.
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
      `BitEncode::bit_encode` for a `Sink`), and construction (struct literal, `builder()`).
- [x] **Verbatim vs canonical encode** — `to_bytes` is verbatim (exactly what's stored;
      byte-identical `decode → to_bytes`); `to_canonical_bytes` normalizes (`reserved` → spec,
      `calc` recomputed). Generated for a `reserved`/`calc` message, alongside the in-memory
      helpers `to_canonical`/`canonical_diff`/`is_canonical`. The form is chosen **per call** —
      there is no carried mode (the `std`-writer `encode(w)` is always verbatim; stream canonical
      via `value.to_canonical().encode(w)`). A `reserved`/`calc` message is an ordinary struct
      (struct literals, serde-compatible). **Struct-only** — a tagged-union enum encodes verbatim
      (no canonical/`validate`).
- [x] **Struct options:** `big`/`little`, `bits = msb|lsb`, `magic = <expr>`
      (sub-byte allowed), `read_only`/`write_only`, `no_builder`, `forward_only`,
      `ctx(name: Ty, …)`, `validate = <path>`.
- [x] **Field directives:** `count`, `ctx { … }`, `temp` + `calc`, `if(…)`,
      `map`/`try_map` (+ inverse `bw(map)`), `parse_with`/`write_with`, `ignore`,
      `pad_*`/`align_*`, `restore_position`, `#[reserved]`/`#[reserved_with(…)]`,
      `#[try_str]` (a `Debug`-rendering hint: a byte buffer prints as a string when valid
      UTF-8, else hex bytes — never lossy; codec unaffected).
- [x] **`WireLen<T>` — auto-deriving, overridable length/count.** A length field that is
      `auto()` (derive at encode, the default) or `set(n)` (explicit override); decode yields
      `Set`, so plain `to_bytes()` is correct-by-default *and* round-trips byte-identically,
      while a forged length survives. `#[bw(auto_len = count(x)|bytes(x))]` (same-struct, element
      or byte length) and `#[bin(auto_len(field.nested = count(source), …))]` (cross-struct,
      the DNS `qdcount`/`rdlength` shape). Checked (no truncation), builder-optional. The
      non-adjacent, byte-length, dual-use counterpart to `count_prefix`; driven by the DNS
      port (the co-evolution "overridable stored-length" gap).
- [x] **Struct-level wire mapping** — a *logical* struct serializes via a separate *wire* type,
      two forms: closures (`map`/`try_map` + `bw_map`) or the conversion traits
      (`wire`/`try_wire` — `From`/`TryFrom<Wire>` decode + `From<&Self>` encode, the transitions in
      named `impl` blocks, reusable in-program). Bypasses the field codec; handles a
      **variable-length** wire form; emits no `FixedBitLen` (a fixed-wire mapped type nests as a
      plain field via a one-line manual impl). See [`bnb::guide::mapping`].
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
- [x] **`Sink::scratch`** — a type-erased, encode-scoped scratch slot (`BitWriter::with_scratch`)
      reachable from any `write_with`/codec fn, for codecs that need mutable state **shared across
      a whole message's fields** — a back-reference / compression dictionary. The sink is already
      the single `&mut` threaded through every field's encode, so a value stored on it is visible
      to them all; recover it with `Any::downcast_mut`. Zero `unsafe`. Surfaced + driven by the DNS
      name-compression port (the co-evolution headline gap).
- [x] `StreamBitReader<R: Read>` — forward-only streaming; `Incomplete` ("read more")
      signal.
- [x] `BufSource<R: Read>` — bounded retain-and-seek socket adapter.
- [x] `BitBuf` — push/pull, bit-aware in-memory buffer: `push(&bytes)` as they arrive,
      `pull::<T>()` takes whole messages off the front (`None` until complete). A `SeekSource`, so
      it also reads through plain `decode`; the pushable counterpart to `BufSource` (`no_std` +
      `alloc`). **Reclaim is deferred + in place** (a push/pull loop reuses one allocation), and a
      **bounded / alloc-once** mode — `BitBuf::bounded(cap)` + `try_push` (`CapacityError` on
      overflow, never reallocates) + explicit `grow` — gives a fixed footprint for real-time/`no_std`.
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
- [x] **Option B** *(decided: post-1.0)* — streaming I/O without `std` is **explicitly not part
      of the 1.0 contract**. The 1.0 `no_std` boundary is buffer-at-a-time plus **`BitBuf`
      push/pull framing** (push bytes from any transport — UART ISR, radio, channel — pull whole
      messages; the bounded/alloc-once mode gives a fixed footprint), which covers the realistic
      embedded case; only can't-fit-in-memory messages and seek-over-stream remain out. When a
      real embedded consumer needs those, the intended mechanism is **adapters over the
      ecosystem's `embedded-io` traits** (the `embedded-hal` family's convergence point), *not*
      an in-house `bnb::io` trait family — direction named so nobody builds the in-house version
      by default; not prototyped. Additive either way.

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

### A. Live testing / dogfooding — **satisfied** (the `RawSocketLabs/protocols` workspace)

The load-bearing gate is met: `bnb` now has a **real, multi-protocol consumer** — the
`RawSocketLabs/protocols` workspace — built from scratch on the codec across three layers.

- [x] **Real protocols on `bnb`, more than the 2–3 asked for** — `link/ethertype` (`BitEnum`
      + catch_all), `application/dns` (the flagship: `#[bitfield]` header, ctx-`tag` `RData`
      union, `WireLen` counts/`rdlength`, `#[bin(codec)]` `Name` with scratch-driven
      compression, refcheck RFC tracking, **plus a UDP/TCP resolver client**), `transport/tcp`
      (`Control` bitfield + structured options), `transport/udp`, and `network/ip` (IPv4
      `#[bitfield]`s). These are **from-scratch** implementations (not ports), each carrying its
      own golden-vector + adversarial suite.
- [x] **Byte-identical golden vectors** — every crate round-trips real wire captures
      byte-for-byte (`decode(wire).to_bytes() == wire`), the codec's real round-trip contract;
      the DNS port fixed the reference crate's 36 misparsed RDATA types along the way.
- [~] **Live interop** — the DNS resolver interops with **live servers** over both UDP and TCP
      (real queries, truncation fallback); the `udp`/`ip` inject layers compose real,
      checksummed datagrams for injection via `rawsock`. Remaining: bulk **pcap** decode of
      captured traffic and a fuzz-a-real-peer pass.

**What dogfooding confirmed:** the feature surface held — `BitEnum`+catch_all, `#[bitfield]` as
a direct RFC bit-diagram, `count = <expr>`, ctx-`tag` unions, `WireLen`, `#[bin(codec)]`
newtypes + `Sink::scratch` all carried real protocols with no workaround. **What it surfaced**
(small, additive — see §C / the co-evolution list): a ready-made `internet_checksum` helper
(the network crates + the `ipv4` example each hand-roll RFC 1071) and `BitDecode`/`BitEncode`
for `std::net::Ipv4Addr`/`Ipv6Addr` (IPv4 models addresses as `u32` today). Neither blocks 1.0.

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
      bit-order matrix at the bitfield layer (`comprehensive.rs` + the combined `bits × bytes`
      case), the message layer (`bin_order_matrix.rs` — the 2×2 compose-without-aliasing), **and
      the low-level cursor** (`bitstream.rs::cursor_layout_matrix`), sub-byte straddles
      (`bitstream_dmr`), and **attacker-controlled `count`** (`bin_count_adversarial.rs`:
      over-count → graceful `UnexpectedEof`, `u32::MAX` count → no pre-alloc, under-count
      → `TrailingBytes`, nested over-read keeps the innermost span).

### C. API freeze + SemVer tooling

- [ ] Deliberate public-API review: trait shapes (`BitDecode`/`BitEncode`/`Source`/
      `Sink`/`Bits`/`Bitfield`), the directive vocabulary, error types, and the
      `EncodeExt::encode(w)` ergonomics — commit only to
      what you'll keep. Mark growth points `#[non_exhaustive]` (errors already are).
      **Scrutinize the encode/construct surface breadth:** a `reserved`/`calc` `#[bin]` struct
      exposes these inherent methods here (`to_bytes`/`to_canonical_bytes` + `to_canonical`/
      `canonical_diff`/`is_canonical` + `builder` + `validate`/
      `is_valid`). The inherent `encode_into`/`canonical_encode_into` sink writers were **cut** as
      redundant over the `BitEncode::bit_encode`/`canonical_bit_encode` trait methods (0 uses vs the
      trait's; sink-composition now uses the trait, symmetric with `encode(w)` needing `EncodeExt`).
      **Decided at the 1.0 freeze: the carried-`encode_mode` mechanism was CUT** — it did not pay for
      its complexity (see the "Encode model" decision below), so the verbatim/canonical choice is now
      purely per-call. That also resolved the two decisions that were coupled to it: **enum
      encode-model parity** (#70 — collapses to a cheap delegating impl, since there's no carried mode
      to mirror) and the **serde-derive boundary on `reserved`/`calc` types** (serde derives now work,
      since there's no longer a non-`Serialize` injected field). Still to weigh before the freeze:
      whether `canonical_diff` earns its slot. **Newer surface to
      scrutinize:** the **two struct-mapping forms** (closures `map`/`bw_map` vs the conversion-trait
      `wire`/`try_wire`) deliver the same capability two ways — keep both or converge? — and the
      `BitBuf` bounded quartet (`bounded`/`try_push`/`grow`/`capacity` + `CapacityError`) is fresh
      surface to confirm earns its place.
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

### Findings from the examples review (a dogfooding proxy)

The examples suite exercises the public API on real formats (DNS, IPv4, AIS, CAN/DBC, WAV, TLV,
…); reviewing it surfaced recurring friction plus one correctness question. Most are **additive**
(they don't block 1.0), but they're the concrete "invest / decide" items real dogfooding
(Section A) should confirm, and they should be weighed *before* the surface freeze (Section C).

- [x] **[shipped] Length-prefixed `count` sugar → `#[brw(count_prefix = <Ty>)]`.** The
      `#[br(temp)] #[bw(calc = self.x.len() as N)] n: N;  #[br(count = n)] x: Vec<T>` triad was the
      single most repeated `#[bin]` idiom; the directive (on the `Vec` itself) now generates the
      whole triad — the prefix sizes the `Vec` on read and is recomputed from `len()` on write
      (derived, never stored). Scope notes: **adjacent-prefix only** (DNS's grouped header counts
      keep the triad — the counts aren't adjacent to their `Vec`s); **element-count semantics**
      (a byte-length prefix — DNS `rdlength` — is a different read loop; a `size_prefix` sibling
      is deferred until a port demands it). Bonuses over the hand-written triad: encode is
      **checked** (`len() > prefix::MAX` is a `BitError::Convert`, where `as u8` silently
      truncated) and **any `Bits` prefix type** works, incl. `uN` (a `u12` prefix is 12 bits on
      the wire — the raw triad's `count = n` can't even compile for a `uN`). Pure desugar, so the
      adversarial-count protections inherit; property-tested byte-identical to the triad
      (`fuzz_roundtrip.rs`), full matrix in `tests/bin_count_prefix.rs`. Adopted by the
      `tlv`/`telemetry`/`bin_message`/`ctx_length` examples.
- [x] **[shipped] `bnb::codecs` — ready-made field codecs.** Three codecs, referenced by path
      (`#[br(parse_with = bnb::codecs::leb128::parse)]`): **`leb128`** (unsigned varint, generic
      over `u8..u128` via the sealed `Varint` marker — the field type pins the width; decode is
      bounded + overflow-checked, fixing the example's unbounded-`shift` debug panic on hostile
      continuation runs; permissive on non-minimal encodings), **`cstring`** (NUL-terminated:
      permissive `Vec<u8>` forms + UTF-8 `String` forms; write rejects an embedded NUL — it could
      not round-trip), and **`prefixed`** (length-prefixed UTF-8 `String`, generic over the
      now-**public sealed `CountPrefix`** — same prefix types as the `count_prefix` directive,
      `uN` included; checked write, no pre-alloc from a hostile prefix). Length-prefixed *bytes*
      deferred to `#[brw(count_prefix = …)]` (same wire form, one attribute); signed
      LEB128/zigzag deferred until a port demands them. Adopted by the `varint`/`cstring`
      examples; `dns` keeps its hand-rolled name codec (compression-pointer chasing is
      DNS-specific — the roll-your-own flagship).
- [x] **[shipped] Reusable *per-type* field codec → codec newtypes + `#[brw(variable)]`.**
      Decided as the **newtype form, not a trait** (a fn pair reused via the type system — no new
      coherence surface, and the field codec was already uniformly `BitDecode`, so the mechanism
      existed; the macro just removes the boilerplate). `#[bin(codec = <module>)]` (the module's
      `parse`/`write`, e.g. `bnb::codecs::leb128`) or `#[bin(codec(parse = <f>, write = <f>))]`
      on a single-field tuple struct generates the delegating `BitDecode`/`BitEncode`, the slice
      entry points, and `From` both ways — annotate once, use as a plain field everywhere
      (`varint`'s `length`/`timestamp` repetition collapses to a bare `Varint` field type). The
      companion **`#[brw(variable)]`** field marker (anticipated by the finding below) suppresses
      the parent's `FixedBitLen` so a variable-length newtype embeds in an otherwise-fixed
      struct — and the missing-marker case is a clear, field-spanned compile error (pinned by
      `ui/bin_codec_needs_variable`). No `FixedBitLen` on the newtype (variable assumed; fixed
      codecs add the manual one-liner). Per-field `parse_with`/`write_with` stays the right tool
      for one-offs. `tests/bin_codec_newtype.rs`; guide § "Per-type codecs".
- [x] **[shipped] A read-side guard directive → `#[br(assert(<expr>[, "fmt", args…]))]`.**
      Decided as (a), the binrw-parity spelling — which is also where AGENTS.md's phantom
      `assert` grammar had come from (aspirational vocabulary from the inspiration crate).
      Runs after the field is read *and mapped* (so it guards the domain value), over this
      and earlier fields; multiple asserts run in order; fails with `ErrorKind::Convert`
      (no new error surface) + the field name + a position. **Read-only** — no `bw` inverse
      (killing the identity-`bw(map)` wart), and encode is untouched, so deliberate forging
      stays possible (the dual-use resolution: the guard is the *explicit opt-in* for values
      unrepresentable in the domain — the same rejection family as `magic`/closed
      enums/`try_map` — while the default parser stays permissive; semantic validity stays
      construction-side in `validate`). The identity-inverse-helper option was rejected as
      still-boilerplate. Pure guards migrated: `versioned`/`versioned_cells` dropped their
      duplicated `check_version` + identity inverses; `checked` stays `try_map` (a genuine
      wire→domain conversion) with the guard-vs-conversion rule cross-referenced. Works on
      struct fields, enum-variant fields, `temp` fields, and through the bare derives
      (shared read path). `tests/bin_assert.rs`; guide § "assert".
- [x] **[decided] Encode-side check for ctx-driven counts → document + `validate`, no codegen.**
      A generated check turned out to be *impossible* for the ctx case — the ctx param does
      not exist at encode time (`to_bytes` takes no context) — and undesirable for stored
      counts: `bin_count_adversarial.rs` deliberately encodes disagreeing counts to forge
      hostile frames, a dual-use feature a mandatory check would break. The crate already
      has the right layer: **construction-side `validate`** (gates `build()`, never the
      parser). Shipped as docs: the guide's `count` section states the obligation (decode
      sizing only; encode trusts the value; consistency is the constructor's job — derive
      the count where the layout allows, `validate` it where it doesn't), `ctx_length`
      demonstrates the `validate`-enforced version (a lopsided row is rejected at
      `build()`), and `versioned_cells` carries the obligation comment at its ctx-driven
      count. Re-open only if the Section A ports show `validate` is not reaching real
      mistakes.
- [x] **[shipped] Bulk byte I/O on `Source`/`Sink` → `read_bytes`/`read_into`/`write_bytes`.**
      Three **defaulted** trait methods (additive; implementations may override with
      byte-aligned fast paths later): `Source::read_bytes(n) -> Vec<u8>` (push-per-byte —
      nothing pre-allocated from the untrusted `n`, a hostile length is a fast EOF),
      `Source::read_into(&mut [u8])` (the no-alloc dual), and `Sink::write_bytes(&[u8])`.
      All work at any bit offset. Adopted internally (`codecs::prefixed`/`cstring`) and in
      the examples — `archive`'s blob loop became one `read_bytes` call (also dropping its
      untrusted `with_capacity`), and its open-coded header size now derives from
      `<Entry as FixedBitLen>::BIT_LEN` (drift-proof); `dns::write_name` labels via
      `write_bytes`. Deliberate public-api addition (+3 methods).
- [x] **[decided] Auto-`FixedBitLen` for fixed-wire mapped types → keep the manual one-liner.**
      Nesting a fixed-wire mapped type as a plain field needs a hand-written
      `impl FixedBitLen { const BIT_LEN = <Wire as FixedBitLen>::BIT_LEN; }` (surfaced building
      `examples/wire_map.rs`), and that stays the mechanism: auto-detection is *impossible* (no
      conditional impls/specialization — the macro can't know whether `Wire: FixedBitLen` holds),
      the one-liner is self-documenting ("fixed *because* the wire is") and fails locally and
      truthfully when the wire is variable. An opt-in `#[bin(wire = W, fixed)]` flag remains a
      purely additive follow-up if the Section A ports show the one-liner is a recurring paper
      cut. Note the interaction with the per-type-field-codec item above: the `#[brw(variable)]`
      field marker (**since shipped**, alongside codec newtypes) attacks the same problem from
      the parent's side and reduces how often `FixedBitLen` matters at all.
- [x] **[correctness · RESOLVED — it was a real bug] LSB × byte-order semantics.** Validating
      against the *specified* DBC-Intel reference (`raw |= v << start; frame = raw.to_le_bytes()`
      — the formula embedded in the tests, not a tool's output) showed `lsb`+`little` did **not**
      match: the old order-agnostic transform ("big = no-op, little = swap") had its meaning
      inverted under LSB, whose natural byte layout is already little-endian. **Fixed** with the
      natural-layout rule (`apply_byte_order` is now bit-order-aware; `Source`/`Sink` grew a
      defaulted `bit_order()`): the identity corners are the two real conventions — `big`+`msb` =
      network order, `little`+`lsb` = DBC-Intel/SMB — and the mixed corners swap. Specified in
      `DESIGN.md`, pinned in `tests/bin_lsb_dbc.rs` (golden + property vs the reference formula),
      and all four corners now golden at the message (`bin_order_matrix`) and cursor
      (`cursor_layout_matrix`) layers; `can_signals` asserts the DBC formula. Round-trips were
      always symmetric — only `lsb` × byte-multiple wire output changed (breaking, on `0.x`,
      zero consumers). The `#[bitfield]` layer already agreed by construction.

### Open decisions to settle before 1.0 (each is a potential breaking change — do on `0.x`)

- [x] **`r` / `w` field-name collision** — *resolved*: the generated source/sink params are
      now `__bnb_r`/`__bnb_w`, so a user field named `r` or `w` no longer collides (the hard
      error is gone). Proof: `bin_macro.rs::fields_named_r_and_w_roundtrip`.
- [x] **Option B (no_std streaming I/O)** *(decided: explicit post-1.0)* — see the `no_std`
      section above: the 1.0 boundary is buffer-at-a-time + `BitBuf` push/pull framing
      (bounded/alloc-once for fixed footprints); future streaming, if demanded, comes as
      additive adapters over `embedded-io`, not an in-house `bnb::io`.
- [x] **Encode model — `calc`/`reserved` handling, verbatim vs canonical** *(done — E1–E3; the
      carried `EncodeMode` shipped and was then removed at the 1.0 freeze — see the marker below)*.
      `to_bytes` used to be an inconsistent hybrid (retained `reserved` but
      recomputed `calc`). **Shipped:**
      - **`to_bytes()` = verbatim** — emit exactly what's stored (retained `reserved` + stored
        non-`temp` `calc`). Matches the `to_bytes`/`as_bytes` ecosystem idiom, is dual-use-honest
        ("never silently rewrite what you gave me"), and restores a byte-identical `decode → to_bytes`
        round-trip as the default. (`temp`+`calc` fields are never stored, so they always recompute.)
      - **`to_canonical_bytes()` = canonical** — normalize `reserved` to its spec value, recompute
        `calc`; always a valid, spec-compliant message. Generated whenever a struct has a `reserved`
        or non-`temp` `calc` field, alongside the in-memory helpers `to_canonical(self) -> Self` /
        `canonical_diff` / `is_canonical`.
      - **Mode carried on the value — Decision (1.0 freeze): REMOVED.** As shipped, a message with a
        `reserved`/`calc` field gained a settable, wire-ignored `encode_mode` field
        (`EncodeMode { Verbatim, Canonical }`, default `Verbatim`) — set via the builder's
        `.encode_mode(…)`, `set_encode_mode`/`with_encode_mode`; read via `encode_mode()`. The
        std-writer `encode(w)` followed it (no `mode` parameter); `to_bytes`/`to_canonical_bytes`
        stayed explicit. The mode was excluded from `PartialEq`/`Eq`/`Hash`/`Debug` (`#[bin]`
        intercepted those derives), so such types were builder/`decode`-constructed (the field
        couldn't appear in a literal). The canonical path was a **defaulted method on `BitEncode`**
        (`canonical_bit_encode`, default = `bit_encode`), so there was **no separate
        `CanonicalEncode`/`CanonicalEncodeExt` trait**. Decided against `read`/`write` naming
        (collides with the `Source::read`/`Sink::write` cursor layer), against a `bool`/Vec-dispatcher,
        and (after first shipping `encode(w, mode)`) against a call-time mode parameter in favor of the
        carried field. **Then, at the 1.0 freeze, the whole carried mechanism was cut** — zero real
        users, and the maintenance cost (the injected wire-ignored field plus the hand-written
        `Debug`/`PartialEq`/`Hash` it forced) wasn't earned; removing it also unblocked serde derives
        and struct literals on `reserved`/`calc` types. **Now:** the verbatim/canonical choice is
        purely **per call** — `to_bytes` vs `to_canonical_bytes` for a `Vec`, and over a writer
        `encode(w)` is always verbatim (stream canonical via `value.to_canonical().encode(w)`).
        `canonical_bit_encode` stays the defaulted `BitEncode` method.
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
- [x] **Encode-model parity for tagged-union enums** *(resolved by the carried-mode removal)* — the
      canonical/`validate` surface stays **struct-only**; a `#[bin]` enum encodes verbatim (the
      boundary is already documented as intentional in the guide/DESIGN: canonical form and validity
      are properties of a *record*; canonicalize the payload type, then dispatch). The old blocker —
      full parity would have needed an enum-side *carried* mode (no field to inject it into, so a
      wrapper or per-variant injection plus per-variant `PartialEq`/`Hash` interception) — is **gone
      now that the carried mode was cut at the C freeze**: with the choice purely per-call, enum
      parity collapses to a cheap delegating `to_canonical_bytes`/`is_canonical` and can be added
      additively if a port ever demands it.
- [x] **LSB × byte-order semantics** *(resolved — specified, validated, and fixed)* — the
      canonical rule is the **natural-layout rule**: each bit order has a natural byte layout
      (big-endian under MSB, little-endian under LSB); the byte-order knob swaps a byte-multiple
      value only when it differs. `little`+`lsb` is now byte-identical to the DBC-Intel reference
      formula (property-tested in `tests/bin_lsb_dbc.rs`); documented in `DESIGN.md`. The fix was
      a breaking wire change for `lsb` × byte-multiple fields, taken on `0.x` with zero consumers
      — see the resolved finding in the examples-review section above.
- [x] **Scope line** *(decided)* — **`serde` and a native async codec are OUT of 1.0 scope.**
      - **serde:** bnb is wire-exact; serde's data model has no bit widths, byte order, magic, or
        count (`binrw` reached the same conclusion) — bnb will not be a serde data format. What
        *is* supported (pinned by `tests/serde_compat.rs`): user-side serde derives coexist with
        **plain** `#[bin]` types, so one type carries both codecs (JSON for config/logs, bnb for
        the wire). Since the carried `encode_mode` field was cut at the 1.0 freeze, this now covers
        **all** `#[bin]` structs — including `reserved`/`calc` messages, which are ordinary structs
        again (the old "serde rejects `reserved`/`calc` messages" boundary is **gone**, along with the
        "no struct literals" limitation it shared a root cause with). bnb's own field types
        (`uN`, bitfields) ship no serde impls (a post-1.0 additive `serde` feature, if demanded).
      - **async:** `BinCodec` over `Framed`/`UdpFramed` (the `tokio` feature) **is** the 1.0
        async story — the codec is in-memory and fast, framing is the async boundary, and
        `BinCodec` covers it. A native async `Source`/`Sink` family is explicitly post-1.0.
