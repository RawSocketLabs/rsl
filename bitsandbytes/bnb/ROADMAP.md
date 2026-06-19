# `bnb` — status and capabilities

**Status: feature-complete.** `bnb` is an owned, bit-aware binary codec — the field
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

## Testing

- [x] Per-directive success tests + the comprehensive bitfield matrix + real protocol
      shapes (DNS/SMB/DMR) + golden byte vectors.
- [x] Property-based round-trips and a robustness suite ("decode of arbitrary bytes
      never panics") across many shapes.
- [x] All runtime error kinds asserted; trybuild compile-fail snapshots for the macro
      misuse surface.
