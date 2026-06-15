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
- [ ] **LSB-first bit order** (today MSB-only) — a `#[bin(bit_order = lsb)]` knob
      (per-struct); mirror `#[bitfield]`'s `bits = msb|lsb`.
- [ ] **Nested `BitDecode` messages** — a `BitDecode` field inside another (the
      derive must call `BitDecode::bit_decode`, not just `Bits::read`, for
      non-`Bits` fields). Resolve the leaf-vs-message dispatch in the derive.
- [ ] **Payload fields** — `Vec<u8>`/`[u8; N]`/`Vec<T: BitDecode>` with a
      length/count source (sets up `count`).
- [x] **Position-aware errors** — carry bit offset + field name in `BitError`
      (the runtime analogue of binrw's error spans).
- [ ] **Coverage** — proptest `encode∘decode = id`; golden vectors for a real
      bit protocol.

**Exit:** a complete DMR *frame* (sync search + slot type + embedded signalling),
not just one burst, parses and round-trips with **no binrw**.

## Phase 2 — The owned `br`/`bw`/`brw` attribute surface

Stop *forwarding* directives to binrw and start *interpreting* them against the
cursor, folding the spike's `#[wire]`/`#[bitwire]` into a single `#[bin]`. Build in
histogram order; each is a checkbox with read + write + a test:

- [ ] `magic` (×214) — read-and-verify / write a constant (bit or byte width).
- [ ] `pre_assert` (×84) — precondition (dual-use: assertion on *construction*/
      opt-in, never a hard parser reject).
- [ ] `big`/`little` (×84) + `bit_order = msb|lsb` (per-struct) — unify with the
      bit codec.
- [ ] `map` / `try_map` (×46) — value transform on read/write.
- [ ] `count` (×32) — count-driven `Vec` (depends on Phase 1 payloads).
- [ ] `ctx` (binrw `args`/`import`, ×25/×19) — parameterized parse, **Layer 1**:
      declare `#[bin(ctx(...))]`, pass `#[br(ctx { … })]`; lower to generated
      **inherent** `Type::decode_with(src, ctx)` + a `Ctx` struct, with the macro
      emitting concrete `decode_with` calls at every field/enum-arm/count-loop. No
      `Args` associated type on the core trait. Covers declarative ASN.1/TLV +
      arbitrary nesting + borrowed context.
- [ ] `ctx` **Layer 2 (deferred, additive)** — a `DecodeWith<A>`/`EncodeWith<A>`
      companion trait (+ blanket `DecodeWith<()>` for every `Decode`) for
      **hand-written generic combinators / trait-object parsing**. Adds the
      polymorphic threading the macro doesn't need; `Type::decode_with` call sites
      are unchanged when it lands, so it can ship later with no churn.
- [ ] `ignore` (×12) — skip on read / don't emit.
- [ ] `calc` / `temp` (×9/×1) — compute-on-write / read-temp (already modeled by
      `#[wire]`'s `#[update]` and group temps; reuse).
- [ ] `parse_with` / `write_with` (×7) — keep as the escape hatch (already the
      bridge primitive).
- [ ] `if` (×3) — conditional field.
- [ ] `restore_position` (×2), `pad_*`/`align_*`, `seek` — position ops with
      **typed** amounts (`N.bits()`/`N.bytes()`, `bnb::prelude`, composable); free on
      the cursor (DD2).
- [ ] `#[reserved]` / `#[reserved = expr]` — explicit reserved members (default
      `0`/`expr`; preserved on decode, settable, count toward fill-exactly). A
      verified-on-read reserved constant is `magic` instead.
- [ ] Directional codecs — `#[bin(read_only)]` / `#[bin(write_only)]` flags
      (read_only ⇒ `Decode` only; write_only ⇒ `Encode` + builder).
- [ ] `validate` — opt-in, **Builder-bound** `fn(&Builder) -> Result<(), impl
      Display>`, run by `build()`; generates `skip_validation()`. Construction
      soundness only; **no** method on the concrete type (supersedes spike §9.4
      post-parse `validate`).

**Exit:** the high-frequency surface is native; the spike's `#[wire]`/`#[bitwire]`
are folded into one `#[bin]`; the binrw bridge is used only for the long tail.

## Phase 3 — The `Source`/`SeekSource` ladder + attribute-driven bounds (DD3/DD4)

The unified I/O model (preview: `design_preview::io_model`). Build the tiers in
order of need:

- [ ] **`Source` trait** — forward byte read + bit-position; impl'd for `&[u8]`
      (consume, transactional) and any `std::io::Read`. `decode(&mut impl Source)`
      works on slice, socket, file for **forward-only** messages.
- [ ] **Attribute-driven bound**: a forward-only message is bounded `Source`; a
      message using a position directive (`seek`/`restore_position`/absolute `pad`)
      is bounded **`SeekSource`**. `#[bin(forward_only)]` pins `Source`-only and
      makes a seek directive a compile error.
- [ ] **`SeekSource: Source`** — adds `seek_to_bit`; impl'd by `BitReader` (slice)
      and (Phase 3b) `R: Read + Seek`. (Inherent `seek_to_bit`/`align_to_byte` stay
      on `BitReader` for the common in-memory case.)
- [ ] **`BufSource<R: Read>`** — the socket+seek adapter: retains read bytes so a
      seek-using message over a *non-seekable* stream works (seek within the
      buffer), reads more on demand, and is **bounded** (`cap(n)`, default = framed
      message size; overflow is an `Err`, never unbounded). The "continuously
      receiving peer that also needs to seek" case.
- [ ] **3b (long-run, not MVP): large seekable files.** `SeekSource for R: Read +
      Seek` so a `File` seeks via `io::Seek` + bit offset, no buffering — the
      file/container-format use case DESIGN §11 DD2 deferred. Designed now (preview
      + this entry), implemented when it earns its place.
- [ ] **Optional `bytes` integration (feature-gated, off by default).** Real value
      for async/tokio networking, but **not** in the core (dependency-light): an
      opt-in `bytes` feature adds `Source`/`Sink` over `Buf`/`BufMut` (zero-copy
      reads from `BytesMut`/`Bytes`/`Chain`) and `Bytes`/`BytesMut` as **zero-copy
      payload** field types (vs the Phase-1 `Vec<u8>`/`[u8; N]`). Pairs with the
      `Incomplete` retry loop for tokio-`Decoder`-style framing. Mirrors how `binrw`
      is feature-gated; users off tokio never pull it in.

**Exit:** forward-only streams need only `Read` (no `NoSeek` tax); seek-over-socket
works bounded via `BufSource`; the large-file `Read + Seek` path is designed and
scheduled.

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
