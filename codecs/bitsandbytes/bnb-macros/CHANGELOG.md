# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries below `0.1.0` are generated from Conventional Commits by release-plz.

## [0.10.1](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-macros-v0.10.0...bitsandbytes-macros-v0.10.1) - 2026-11-13

### Added

- *(bitsandbytes)* bound decode recursion with a depth budget

### Other

- *(bitsandbytes)* decode a fixed-length message from one window
- *(bitsandbytes)* size variable-length to_bytes before writing
- *(bitsandbytes)* dispatch byte-string magics from one look

## [0.10.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-macros-v0.9.0...bitsandbytes-macros-v0.10.0) - 2026-11-11

### Fixed

- *(bitsandbytes)* correct the new macro diagnostics after review
- *(bitsandbytes)* name the generated item a field's accessors collide with
- *(bitsandbytes)* read a field type through a macro_rules type fragment
- *(bitsandbytes)* keep generated names out of the user's namespace
- *(bitsandbytes)* stop three inputs from panicking the macros
- *(bitsandbytes)* ask nothing of the direction a narrowed type drops
- *(bitsandbytes)* build a validate hook's error message without std
- *(bitsandbytes)* [**breaking**] reject container options the item kind would ignore
- *(bitsandbytes)* [**breaking**] reject conflicting and duplicate field directives
- *(bitsandbytes)* measure an auto_len bytes source with its own write
- *(bitsandbytes)* encode enum variant fields with the struct write generator
- *(bitsandbytes)* [**breaking**] end decode_all and decode_iter when a message does not advance

### Other

- *(bitsandbytes)* generate the slice entry points from one place
- *(bitsandbytes)* split the codec macro into modules
- *(bitsandbytes)* allocate a fixed-length message's bytes once
- *(bitsandbytes)* remove an unused private export and two duplicates
- *(bitsandbytes)* correct documentation that no longer matches the code
- *(bitsandbytes)* state that canonical encode covers a message's own fields

## [0.9.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-macros-v0.8.0...bitsandbytes-macros-v0.9.0) - 2026-11-10

### Added

- *(bitsandbytes)* support named context tags and aliases

### Fixed

- *(bitsandbytes)* [**breaking**] give ctx enums that never read context a plain to_bytes
- *(bitsandbytes)* [**breaking**] close the review's five correctness findings

### Other

- *(bitsandbytes)* bring the docs up to 0.8.0
- describe the codecs directory as part of the rsl workspace

### Migration

- A `#[bitfield]` field whose `#[bits(N)]` slot is wider than its type's `Bits::BITS` no longer
  compiles (a `BitEnum` getter used to panic on the extra bits). Narrow the slot or widen the type.
- `BufSource` returns `ErrorKind::UnexpectedEof`, not `Incomplete`, for a read past its reader's
  EOF. Match `UnexpectedEof` to detect a clean close.
- A nested `#[bitfield]`/`#[bitflags]` of 16 bits or more is read and written in its own byte
  order (declared, or the default big) wherever it is nested, so inside a message of the other
  order its wire bytes are now its standalone `to_bytes`. Declare `bytes = …` to match the message
  to keep the old bytes.
- `decode_all`/`decode_iter` treat a sub-byte tail that runs out or is all zero as padding, so
  they return `Ok` where they used to error.
- `BitWriter::write_bits`/`write_bytes` and `StreamBitReader::read_bits` can return
  `PositionOverflow` once a position no longer fits `usize` (32-bit targets).
- A `#[bin(ctx(...))]` enum whose only encode-side mentions of a ctx parameter are same-named
  variant fields (or `ctx { name = source }` alias labels) now gets a plain `BitEncode` and
  `to_bytes`, and no longer has the inherent `encode_with`/`to_bytes_with`; its `EncodeWith` impl
  remains. Call `to_bytes()` or `EncodeWith::encode_with`, and delete any hand-written
  `BitEncode` impl for such a type, which now conflicts with the generated one.

## [0.8.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-macros-v0.7.1...bitsandbytes-macros-v0.8.0) - 2026-10-08

### Added

- *(bitsandbytes)* [**breaking**] emit From conversions for #[bitfield] over its backing integer ([#115](https://github.com/RawSocketLabs/rsl/pull/115))
- *(bitsandbytes)* add stateless slice prefix decoding ([#108](https://github.com/RawSocketLabs/rsl/pull/108))

### Other

- *(bitsandbytes)* copy byte-aligned byte runs in bulk ([#109](https://github.com/RawSocketLabs/rsl/pull/109))

## [0.7.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-macros-v0.5.0...bitsandbytes-macros-v0.7.0) - 2026-09-21

### Added

- *(bitsandbytes)* [**breaking**] attribute closed-enum dispatch misses by type ([#87](https://github.com/RawSocketLabs/rsl/pull/87))

## [0.5.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-macros-v0.4.0...bitsandbytes-macros-v0.5.0) - 2026-09-09

### Added

- *(bitsandbytes)* [**breaking**] support incremental decoding and lossless handoff

## [0.4.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-macros-v0.3.2...bitsandbytes-macros-v0.4.0) - 2026-09-07

### Added

- *(bitsandbytes)* [**breaking**] normalize builder enum aliases and prepare release

## [0.3.2](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-macros-v0.3.1...bitsandbytes-macros-v0.3.2) - 2026-07-24

### Added

- *(bnb-macros)* add the #[view(const)] assertion
- *(bnb-macros)* keep the value-bearing panic on the runtime decode path
- *(bnb-macros)* make generated accessors const fn, emit repr(transparent)

## [0.3.1](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-macros-v0.3.0...bitsandbytes-macros-v0.3.1) - 2026-07-23

### Added

- *(bnb)* add contextual #[view] fields to #[bitfield]
- *(bnb)* add read-side #[br(calc = <expr>)]

## [0.3.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-macros-v0.2.0...bitsandbytes-macros-v0.3.0) - 2026-07-06

### Other

- *(bnb)* polish the guide + macro reference for 1.0 ([#30](https://github.com/RawSocketLabs/rsl/pull/30))
- *(bnb)* [**breaking**] harden the public surface for the freeze ([#26](https://github.com/RawSocketLabs/rsl/pull/26))
- *(bnb)* [**breaking**] pre-freeze naming fixes (Error, codec, raw, with_cap) ([#25](https://github.com/RawSocketLabs/rsl/pull/25))
- *(bnb)* [**breaking**] unify the byte/bit-order vocabulary across all macros ([#23](https://github.com/RawSocketLabs/rsl/pull/23))
- *(bnb)* [**breaking**] cut the carried encode_mode mechanism ([#22](https://github.com/RawSocketLabs/rsl/pull/22))

## [0.2.0](https://github.com/RawSocketLabs/bitsandbytes/compare/bitsandbytes-macros-v0.1.0...bitsandbytes-macros-v0.2.0) - 2026-06-22

### Added

- *(bin)* re-runnable validate() / is_valid() methods ([#21](https://github.com/RawSocketLabs/bitsandbytes/pull/21))
- *(bin)* [**breaking**] carry encode mode on the value; encode(w) follows it ([#20](https://github.com/RawSocketLabs/bitsandbytes/pull/20))
- *(bin)* canonical helpers — to_canonical / canonical_diff / is_canonical ([#16](https://github.com/RawSocketLabs/bitsandbytes/pull/16))
- *(bitfield)* custom Debug that decomposes the logical fields ([#15](https://github.com/RawSocketLabs/bitsandbytes/pull/15))

### Other

- *(bin)* [**breaking**] cut inherent encode_into / canonical_encode_into ([#23](https://github.com/RawSocketLabs/bitsandbytes/pull/23))
- clarify the verbatim vs canonical encode model ([#19](https://github.com/RawSocketLabs/bitsandbytes/pull/19))
- [**breaking**] encode(w, mode: EncodeMode) — fold canonical into BitEncode ([#18](https://github.com/RawSocketLabs/bitsandbytes/pull/18))
- [**breaking**] split encode into verbatim (to_bytes) + canonical (to_canonical_bytes) ([#14](https://github.com/RawSocketLabs/bitsandbytes/pull/14))
- *(bnb-macros)* rename codec source/sink params to __bnb_r/__bnb_w ([#1](https://github.com/RawSocketLabs/bitsandbytes/pull/1))

## [0.1.0] - 2026-06-19

Initial baseline of the `bitsandbytes-macros` proc-macro crate.
