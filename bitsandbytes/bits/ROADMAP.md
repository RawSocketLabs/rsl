# Roadmap — rebuilding binrw's capabilities into `bnb`

> Goal: an **owned, bit-aware binary codec** that subsumes what we use from
> `binrw`, so the crate (renamed `bnb` / `bnb-macros`, `bnb::bin`) provides both
> the bit layer *and* the byte layer, and the external `binrw` dependency can be
> dropped. Design rationale: `DESIGN.md` §4 (build-vs-buy), §10 (bit-codec spike),
> §11 (confirmed decisions DD1–DD5). Credit: `ACKNOWLEDGMENTS.md`.

## Guiding principles

1. **Keep the binrw bridge until we reach parity.** Every phase ships working;
   `binrw` stays a (default-on) feature and the `#[bitwire]`/`#[wire]` dispatch
   keeps using it for anything we haven't rebuilt. No big-bang rewrite.
2. **Sequence by real usage**, not feature completeness. The workspace attribute
   histogram (DESIGN §9.1) is the priority list: `magic ×214`, `pre_assert ×84`,
   `big/little ×84`, `map ×46`, `count ×32`, `args ×25`, `import ×19`,
   `ignore ×12`, `calc ×9`, `parse_with ×7`, `if ×3`, `restore_position ×2`,
   `temp ×1`. Build the head of that distribution first; the long tail can stay
   bridged or be dropped.
3. **One vocabulary (`br`/`bw`/`brw`), semantically faithful to binrw** (DD1).
   Where we reuse a spelling it must mean what binrw means.
4. **Dual-use is non-negotiable** (workspace rule): parsers never reject
   representable input; validation/soundness stays construction-time and opt-in.
5. **Seek-free by default; bounds are attribute-driven** (DD2/DD3): in-memory
   cursor needs no `Seek`; a streaming backend requires `Seek` only where a
   position-dependent directive actually uses it.
6. **Each phase carries its tests**: unit + golden vectors + proptest round-trips
   + trybuild misuse snapshots + a criterion bench vs. the binrw path.

## Feature workflow (how each roadmap item ships)

A repeatable loop; every checkbox below goes through it before it is ticked:

1. **Scope** — read the `design_preview` + this entry; identify what it touches
   (runtime `bits/src/bitstream.rs` / macro `bits-macros/src/bitstream.rs` / tests).
2. **Plan** the smallest chunk; build **runtime helper → macro codegen → test**.
3. **Implement** incrementally; keep the macro emitting *const exprs / helper calls*,
   never computing widths itself; pin generated types so inference can't drift.
4. **Prove** — a positive round-trip/golden test **and** a `trybuild` negative
   wherever there is a misuse to catch (clear, well-spanned error).
5. **Gate** (all clean): `RUSTC_WRAPPER= cargo test -p bits`, `… --no-default-features`,
   `… clippy -p bits-macros -p bits --all-targets` (`clippy::all = deny`),
   `… fmt --all --check`.
6. **Document** — tick this item with a one-line record of the design decision;
   refresh the module / derive docs and the "which macro when?" table.
7. **Commit** — one Conventional-Commit per item (`feat`/`fix`/`refactor` + scope,
   `!` for breaking); the body explains the *why*.
8. **Remember** — record non-obvious decisions in agent memory.

## Phase 0 — Spike *(done)*

`BitReader`/`BitWriter` (MSB-first slice cursors), `#[derive(BitDecode/BitEncode)]`,
the right-tool guard (`#[bit_stream(allow_byte_aligned)]`), `seek_to_bit`/
`align_to_byte`, the forward-only `StreamBitReader<R: Read>` (DD3 demo), and
`#[bitwire]` dispatch (binrw for byte-aligned fields, bit cursor for a `#[bits]`
region, via binrw's `parse_with`/`write_with`). Proofs: `tests/bitstream_dmr.rs`,
`tests/bitstream_seek.rs`, `tests/bitwire_dispatch.rs`, `tests/bitstream_guard.rs`
+ `tests/ui/bitstream_byte_aligned.rs`.

## Phase 1 — Core bit codec hardening

Make the bit codec able to express a *whole* message, not just a fixed region.

- [x] **Entry points + builder** — `decode(&mut impl Source)` / `decode_exact` /
      `peek` / `decode_from`; `encode(&mut impl Sink)` / `to_bytes` / `encode_into`;
      and the required-by-default builder. `Source`/`Sink` start as `&[u8]` +
      `Read`/`Write`; the seek ladder is Phase 3. `Incomplete { needed: Option<usize> }`
      streaming signal.
- [x] **LSB-first bit order** — `#[bit_stream(bit_order = lsb)]` (per-struct; the
      `#[bin(...)]` spelling arrives with the Phase 2 macro). BitReader/BitWriter are
      order-aware; order flows through nesting via Source/Sink. `StreamBitReader`
      LSB + mixed-order nesting are Phase 2.
- [x] **Nested `BitDecode` messages** — a `BitDecode` field inside another (the
      derive must call `BitDecode::bit_decode`, not just `Bits::read`, for
      non-`Bits` fields). Resolve the leaf-vs-message dispatch in the derive.
- [x] **Fixed payload fields** — `[u8; N]` byte arrays (read/written even at a
      non-byte-aligned offset; `N * 8` toward `BIT_LEN`). Variable `Vec<u8>`/
      `Vec<T>` + `count` are **Phase 2** (where the `count` attribute lives), since
      they break the const `BIT_LEN` and need the `FixedLen`-trait split.
- [x] **Position-aware errors** — carry bit offset + field name in `BitError`
      (the runtime analogue of binrw's error spans).
- [x] **Coverage** — proptest `encode∘decode = id` over random field values; a
      golden byte vector; known-sync recognition + unknown-sync preservation
      (`tests/bitstream_dmr_frame.rs`).

**Exit ✓ (achieved):** a complete DMR *frame* — slot type + a 264-bit nested
burst (with a 48-bit sync `BitEnum`) + a CRC payload — round-trips with **no
binrw** (`tests/bitstream_dmr_frame.rs`).

> **Phase 1 deferrals → Phase 2** (refinement, as agreed): drop the `#[nested]`
> marker via universal `Bits` impls; variable `Vec`/`count` payloads (with the
> `BIT_LEN`→`FixedLen` split); `StreamBitReader` LSB + mixed-order nesting; dotted
> error paths (`outer.inner.leaf`); the `bytes` feature (Phase 3).

## Phase 2 — The owned `br`/`bw`/`brw` attribute surface

Stop *forwarding* directives to binrw and start *interpreting* them against the
cursor, folding the spike's `#[wire]`/`#[bitwire]` into a single `#[bin]`. Build in
histogram order; each is a checkbox with read + write + a test:

- [x] **Foundation: `#[bin]` macro** — one attribute folding codec + builder
      (`read_only`/`write_only`/`no_builder`/`bit_order`/`allow_byte_aligned`),
      lowering to `#[derive(BitDecode, BitEncode, BitsBuilder)]` + `#[bit_stream]`.
      The directives below ride through as derive helper attributes.
- [x] `magic` (×214) — read-and-verify / write a constant (bit or byte width).
      `#[bin(magic = <expr>)]`; sub-byte allowed (`u3::new(0b110)`, beyond binrw) so
      it suppresses the right-tool guard; mismatch → `ErrorKind::BadMagic`.
      `tests/bin_magic.rs`.
- [x] `pre_assert` (×84) — realized as `#[bin(validate = <path>)]`: a free
      `fn(&Self) -> Result<(), impl Display>` run by `build()` (a failure →
      `BuilderError::Invalid`). Dual-use: the **parser stays permissive** — `decode`
      never validates, so a non-conformant value still parses; the struct literal is
      the raw escape. Folded `#[bin]`'s builder onto `builder::generate` (post_build
      hook) instead of `#[derive(BitsBuilder)]`. `tests/bin_validate.rs` +
      `ui/bin_validate_needs_builder`. (`skip_validation()` convenience deferred —
      the literal already bypasses; binrw's *read-side* pre_assert stays a non-goal.)
- [x] `big`/`little` (×84) + `bit_order = msb|lsb` (per-struct) — a `Layout` (bit +
      byte order) threads through the cursors and `Source`/`Sink` (so it flows through
      nesting/map/magic). `#[bin(little)]` byte-swaps **byte-multiple** values (binrw's
      rule); sub-byte/straddling widths are unaffected. `apply_byte_order` is its own
      inverse (read/write share it). `tests/bin_byte_order.rs` (wire-visible golden).
- [x] `map` / `try_map` (×46) — `#[br(map = <f>)]` reads the wire value (`f`'s arg
      type) and transforms it to the field type; `#[br(try_map = <f>)]` is fallible
      (a conversion error → `ErrorKind::Convert`). `#[bw(map = <f>)]` is the inverse
      (write `f(&self.field)`); a read-side map without it is a clear error. A mapped
      field's type isn't `Bits`, so it's variable + guard-exempt. `tests/bin_map.rs`
      + `ui/bin_map_needs_inverse`.
- [x] `count` (×32) — `#[br(count = <expr>)]` on a `Vec<T>` (leaf or `#[nested]`
      elements); `expr` may name an earlier field. Forced the `BIT_LEN`→`FixedBitLen`
      split: `BitDecode` drops the const; a fixed message *also* impls `FixedBitLen`,
      a `count`-bearing one does not (trybuild `ui/bin_count_not_fixed`). Reads grow
      the `Vec` without untrusted pre-allocation. `tests/bin_count.rs`. (Pairing with
      `temp`/`calc` so the length field isn't stored: the `calc`/`temp` chunk.)
- [x] `ctx` (binrw `args`/`import`, ×25/×19) — parameterized parse, **Layer 1**:
      declare `#[bin(ctx(name: Ty, …))]`, pass `#[br(ctx { a, b })]`. A ctx type gets
      inherent `decode_with`/`encode_with`/`decode_with_exact`/`to_bytes_with` + a
      generated `<Name>Ctx` struct (emitted by `#[bin]`), and does **not** implement
      `BitDecode`/`BitEncode` (no `Args` on the core trait; trybuild
      `ui/bin_ctx_needs_context`). The macro emits concrete `decode_with`/`encode_with`
      calls at every ctx field and count-loop element; passed names resolve per
      direction (a parent field → `self.x` on encode, a parent ctx param → local).
      Covers TLV/ASN.1 + Vec-of-ctx. `tests/bin_ctx.rs`. (Enum arms: once `#[bin]`
      enums land.)
- [x] `ctx` **Layer 2 (additive)** — `DecodeWith<A>`/`EncodeWith<A>` companion traits
      with a blanket `DecodeWith<()>` for every `BitDecode` (and `EncodeWith<()>` for
      every `BitEncode`); a `#[bin(ctx(...))]` type also gets `DecodeWith<…Ctx>`
      (delegating to its inherent `decode_with`). So one bound `T: DecodeWith<A>`
      spans context-free and context-taking messages — for generic combinators /
      trait-object parsing. Inherent call sites unchanged. `tests/bin_ctx_layer2.rs`.
- [x] `ignore` (×12) — `#[br(ignore)]`: an in-memory-only field, `Default::default()`
      on read (no input consumed) and skipped on write (zero wire bits), but still a
      stored + builder field. Excluded from `BIT_LEN`/the guard. `tests/bin_ignore.rs`.
- [x] `calc` / `temp` (×9/×1) — `#[br(temp)]` reads into a local (usable by a later
      `count`/`ctx`) but is **not stored**; `#[bw(calc = <expr>)]` writes a value
      computed from the other fields (pinned to the field's type). Together they drop
      a redundant length/count field from the struct *and* the builder, and keep it
      from drifting from the `Vec` (matched read/write, generated together). This
      forced `#[bin]` to generate the codec **directly** (extracted `gen_decode`/
      `gen_encode`, shared with the derives) instead of lowering — so a `temp` field
      absent from the emitted struct can still drive the codec. `tests/bin_calc_temp.rs`
      + `ui/bin_temp_needs_calc`.
- [x] `parse_with` / `write_with` (×7) — the field-level custom-codec escape hatch,
      now **native** (no binrw): `#[br(parse_with = f)]` reads via `f(r) -> Result<T,
      BitError>` and `#[bw(write_with = f)]` writes via `f(&self.field, w)`. A
      parse_with without its inverse is a clear error; the field is treated as
      custom-width (guard-exempt). `tests/bin_parse_with.rs`.
- [x] `if` (×3) — `#[br(if(<cond>))]` on an `Option<T>` field: `Some(read)` when the
      condition (over earlier fields, as locals) holds, else `None`; on encode the
      `Option`'s presence drives the write (the read condition isn't re-evaluated).
      Leaf or `#[nested]` inner. A non-`Option` field is a clear error. The `#[br]`
      parser is now keyword-aware (`if` is a keyword, so `parse_nested_meta` can't
      read it). `tests/bin_if.rs` + `ui/bin_if_needs_option`.
- [x] `pad_*`/`align_*` — forward positioning with **typed** amounts (`4.bits()` /
      `3.bytes()` via `bits::prelude`): `#[br(pad_before/pad_after = <bits>)]` skips a
      bit count, `#[br(align_before/align_after)]` skips to the next byte boundary.
      Works on any forward `Source` (skip = read-and-discard / write zeros); such a
      field is guard-exempt. `tests/bin_positioning.rs`. **Backward `seek` /
      `restore_position` (×2) are deferred to Phase 3** — they need `SeekSource`
      (DD2/DD3); the in-memory `BitReader::seek_to_bit` already exists for direct use.
- [x] `#[reserved]` / `#[reserved_with(<expr>)]` — reserved bits: on the wire (the
      field type gives the width, so they count toward `BIT_LEN`/the guard) but not
      stored. Read and discarded (lenient — a non-zero value isn't rejected; use
      `magic` for an enforced constant); written as the type's zero / `<expr>`.
      Dropped from the struct and builder (like `temp`, but auto-written). `#[bin]`
      only. `tests/bin_reserved.rs`.
- [x] Directional codecs — `#[bin(read_only)]` / `#[bin(write_only)]` flags
      (read_only ⇒ `Decode` only, no builder; write_only ⇒ `Encode` + builder).
      Shipped with the `#[bin]` foundation; mutually exclusive (a clear error).
- [x] `validate` — shipped as the `pre_assert` item above (`#[bin(validate = path)]`,
      run by `build()`, construction-soundness only, **no** method on the concrete
      type). Implemented as a free `fn(&Self) -> Result<(), impl Display>` rather than
      `fn(&Builder)` — it validates the fully-resolved value (a builder with `Option`
      fields is awkward to check, and `self` is partly moved by then). `skip_validation()`
      stays deferred: the struct literal already bypasses (the dual-use raw path).

- [x] **Fold `#[wire]`/`#[bitwire]` into `#[bin]`** — `#[bin]` is the unified codec:
      the right-tool guard is suppressed for it (kept as advisory steering on the bare
      derives), so a **byte-aligned** message is a first-class `#[bin]` use, covering
      everything `#[wire]` did natively (magic, `temp`/`calc` count, count-driven `Vec`,
      builder — no binrw). `tests/bin_fold.rs`. `#[wire]`/`#[bitwire]` are kept for
      binrw interop only (dropped in Phase 4) and documented as superseded.

**Exit ✓ (achieved):** the high-frequency surface is native; the spike's
`#[wire]`/`#[bitwire]` are folded into one `#[bin]`; the binrw bridge is used only
for the long tail (and Phase 4 removes it from the default graph).

## Phase 3 — The `Source`/`SeekSource` ladder + attribute-driven bounds (DD3/DD4)

The unified I/O model (preview: `design_preview::io_model`). Build the tiers in
order of need:

- [x] **`Source` trait** — forward bit read + bit-position; impl'd by `BitReader`
      (slice) and `StreamBitReader<R: Read>`. `decode_from(&mut impl Source)` works
      forward-only; `decode`/`peek`/`decode_exact` wrap a slice. (Shipped Phase 1.)
- [x] **Attribute-driven bound**: a forward-only message uses `Source`;
      `restore_position` rewinds via `Source::seek_to_bit` (real on a `SeekSource`,
      else a runtime `ErrorKind::NotSeekable`). `#[bin(forward_only)]` makes a seek
      directive (`restore_position`) a **compile error** (trybuild
      `ui/bin_forward_only_no_seek`). `tests/bin_restore_position.rs`.
- [x] **`SeekSource: Source`** — a marker guaranteeing `seek_to_bit` works; impl'd
      by `BitReader` (slice). (`R: Read + Seek` adapter is 3b.) Inherent
      `seek_to_bit`/`align_to_byte` stay on `BitReader` for the in-memory case.
- [x] **`BufSource<R: Read>`** — the socket+seek adapter: retains read bytes so a
      seek-using message over a *non-seekable* stream works (seek within the buffer),
      reads more on demand, and is **bounded** (`cap`, default 64 KiB; overflow →
      `ErrorKind::BufferFull`, never unbounded). A `SeekSource`. `tests/bin_buf_source.rs`.
- [x] **3b: large seekable files.** `SeekReader<R: Read + Seek>` — a `SeekSource`
      that seeks via `io::Seek` to the byte holding the bit cursor, no buffering (the
      file/container-format case). `tests/bin_seek_reader.rs`.
- [x] **Optional `bytes` integration (feature-gated, off by default).** The `bytes`
      feature adds `BytesReader` (a `SeekSource` that **owns** a `Bytes` frame — a
      refcount-bump construct) and `BytesWriter` (a `Sink` that `freeze()`s to a
      zero-copy `Bytes`) — the async/tokio framing path, pairing with the `Incomplete`
      retry loop. Off by default so the core stays dependency-light. `tests/bin_bytes.rs`.
      (`Bytes`/`BytesMut` as zero-copy *payload field types* in the macro are an
      additive follow-on when a tokio consumer lands.)

**Exit ✓ (achieved):** forward-only streams need only `Source` (no `NoSeek` tax);
seek-over-socket works bounded via `BufSource`; the large-file `Read + Seek` path
(`SeekReader`) and the opt-in `bytes` adapters are implemented.

## Phase 4 — Reach parity, drop the binrw dependency

- [ ] Audit remaining binrw-bridged call sites; rebuild or consciously drop the
      long tail.
- [ ] Move `binrw` behind an **optional `binrw-compat`** feature (interop only),
      default **off**; the native codec is the default path.
- [ ] Remove `binrw`/`binrw_derive` from the default dependency graph; update
      `deny.toml`, `Cargo.lock`, license story. Keep `ACKNOWLEDGMENTS.md`.

**Exit:** `cargo tree` shows no default `binrw`; all `bits`/protocol tests pass.

## Phase 5 — Rename `bits` → `bnb`

Mechanical, once the codec is genuinely owned (DD5):

- [ ] Rename crates `bits`→`bnb`, `bits-macros`→`bnb-macros`; `bnb::bin`.
- [ ] Update generated-code paths (`::bits::__private::*` → `::bnb::…`), all
      member `Cargo.toml`s, `[workspace.dependencies]`, `deny.toml`, refcheck, and
      the root `AGENTS.md` crate table.
- [ ] One commit, full CI green; update memory + docs.

## Cross-cutting (every phase)

- **Migration pilots:** validate parity on one real **bit** protocol (DMR-style)
  and one **byte** protocol (DNS via `#[bin]`, byte-identical golden) before any
  broad protocol-crate adoption. Don't migrate protocol crates onto the codec
  until its phase's exit criteria are met.
- **Bench:** criterion vs. the binrw path (`testutil::bench`), watch for
  regressions; the bit cursor must stay shift/mask-fast (no `bitvec`).
- **Lints/MSRV:** stay clean under `clippy::all = deny`; keep `#![deny(missing_docs)]`.
- **Docs:** the "which macro when?" decision table stays current as the spike's
  `#[wire]`/`#[bitwire]` merge into `#[bin]`.

## Risks & watch-items

- **Error-span quality** when our macro owns the surface (don't regress binrw's
  good diagnostics — emit field tokens with original spans, trybuild-guard misuse).
- **`args`/`import` generality** is the most complex slice of binrw; timebox it,
  keep `parse_with` as the escape hatch so we never *must* model everything.
- **Scope creep** — the long tail of binrw is large; principle #2 says we are
  allowed to leave parts bridged or unbuilt indefinitely. Parity means "covers our
  protocols," not "reimplements binrw 1:1."
