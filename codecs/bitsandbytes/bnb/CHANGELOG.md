# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries below `0.1.0` are generated from Conventional Commits by release-plz.

## [0.10.1](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.10.0...bitsandbytes-v0.10.1) - 2026-11-13

### Added

- *(bitsandbytes)* cap BinCodec frames with with_max_frame
- *(bitsandbytes)* bound decode recursion with a depth budget

### Other

- *(bitsandbytes)* record the magic fallthrough and close an enum item
- *(bitsandbytes)* dispatch byte-string magics from one look
- *(bitsandbytes)* probe magic dispatch through a buffer and on short input
- *(bitsandbytes)* record the rest of phase 2
- *(bitsandbytes)* inline BytesReader's typed read
- *(bitsandbytes)* read through a window at the wrapped reader's speed
- *(bitsandbytes)* probe windowed and BytesReader decodes
- *(bitsandbytes)* record phase 2 of the happy-path plan
- *(bitsandbytes)* inline the incremental reader's byte-run read
- *(bitsandbytes)* inline typed reads on the slice readers
- *(bitsandbytes)* record the performance research results
- *(bitsandbytes)* record the dispatch-miss tier decision
- *(bitsandbytes)* record phase 1 of the performance plan
- *(bitsandbytes)* inline the per-read helpers of the net readers
- *(bitsandbytes)* load an unaligned read's window whole
- *(bitsandbytes)* build the attempt's reader in place
- *(bitsandbytes)* probe unaligned reads by width and place
- *(bitsandbytes)* add a fixed-work probe with tiered modes
- *(bitsandbytes)* record the happy-path performance review and its plan
- *(bitsandbytes)* tick the 0.10.0 baseline reset in the roadmap

## [0.10.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.9.0...bitsandbytes-v0.10.0) - 2026-11-11

### Added

- *(bitsandbytes)* [**breaking**] convert any primitive integer into a UInt with a range check
- *(bitsandbytes)* make BinCodec Clone, Copy and Debug

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
- *(bitsandbytes)* retry an interrupted read in BufSource
- *(bitsandbytes)* let a BufSource release consumed bytes with compact
- *(bitsandbytes)* [**breaking**] end decode_all and decode_iter when a message does not advance
- *(bitsandbytes)* [**breaking**] require Send scratch so writers can cross threads

### Other

- *(bitsandbytes)* record 0.10.0 qualification and plan 0.10.x and 0.11
- qualify the bitsandbytes 0.10.0 candidate against 0.9.0 in major mode
- *(bitsandbytes)* add an upgrade page for 0.9 to 0.10
- *(bitsandbytes)* record the whole-crate review, its fixes and measurements
- *(bitsandbytes)* prove MessageStream reads through the scratch it keeps
- *(bitsandbytes)* benchmark mixed-width magic dispatch
- *(bitsandbytes)* keep one read scratch per MessageStream
- *(bitsandbytes)* convert byte-aligned integer runs in one pass
- *(bitsandbytes)* allocate a fixed-length message's bytes once
- *(bitsandbytes)* read and write unaligned fields a byte at a time
- *(bitsandbytes)* stop the bitfield benches folding their input
- *(bitsandbytes)* pin the behaviour six surviving mutants changed
- *(bitsandbytes)* state the recursion and frame-size limits a caller must bound
- *(bitsandbytes)* remove peek_bytes, unused since the 0.5.0 macros
- *(bitsandbytes)* remove an unused private export and two duplicates
- *(bitsandbytes)* correct documentation that no longer matches the code
- *(bitsandbytes)* state that canonical encode covers a message's own fields

## [0.9.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.8.0...bitsandbytes-v0.9.0) - 2026-11-10

### Added

- *(bitsandbytes)* support named context tags and aliases

### Fixed

- *(bitsandbytes)* [**breaking**] give ctx enums that never read context a plain to_bytes
- *(bitsandbytes)* [**breaking**] close the review's five correctness findings

### Other

- *(bitsandbytes)* pin the LSB padding mask and record release evidence
- restore CI facts and settle the review's nits
- *(bitsandbytes)* bring the docs up to 0.8.0
- describe the codecs directory as part of the rsl workspace
- replace GitHub Actions and act with CircleCI and a local runner on one pinned image
- drop the rsl- prefix from crate names
- move crates into domain groups

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

## [0.8.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.7.1...bitsandbytes-v0.8.0) - 2026-10-08

### Added

- *(bitsandbytes)* [**breaking**] emit From conversions for #[bitfield] over its backing integer ([#115](https://github.com/RawSocketLabs/rsl/pull/115))
- *(bitsandbytes)* add stateless slice prefix decoding ([#108](https://github.com/RawSocketLabs/rsl/pull/108))

### Other

- *(bitsandbytes)* track incremental-read helper overhead ([#106](https://github.com/RawSocketLabs/rsl/pull/106))
- *(bitsandbytes)* copy byte-aligned byte runs in bulk ([#109](https://github.com/RawSocketLabs/rsl/pull/109))
- satisfy Rust 1.99's new clippy and rustdoc lints ([#99](https://github.com/RawSocketLabs/rsl/pull/99))

## [0.7.1](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.7.0...bitsandbytes-v0.7.1) - 2026-09-25

### Added

- *(bitsandbytes)* add BufSeekReader, a buffered seek reader for owned inputs ([#94](https://github.com/RawSocketLabs/rsl/pull/94))

### Other

- *(bitsandbytes)* record verified 0.4.1 delivery ([#92](https://github.com/RawSocketLabs/rsl/pull/92))

## [0.7.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.6.0...bitsandbytes-v0.7.0) - 2026-09-21

### Added

- *(bitsandbytes)* [**breaking**] attribute closed-enum dispatch misses by type ([#87](https://github.com/RawSocketLabs/rsl/pull/87))

### Other

- select affected package and downstream validation ([#83](https://github.com/RawSocketLabs/rsl/pull/83))
- *(bitsandbytes)* enforce published 0.6 compatibility

## [0.6.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.5.0...bitsandbytes-v0.6.0) - 2026-09-09

### Added

- *(bitsandbytes)* [**breaking**] honor decode hints in whole-message reads

### Other

- *(bitsandbytes)* enforce published 0.5 compatibility

## [0.5.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.4.1...bitsandbytes-v0.5.0) - 2026-09-09

### Added

- *(bitsandbytes)* [**breaking**] support incremental decoding and lossless handoff

## [0.4.1](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.4.0...bitsandbytes-v0.4.1) - 2026-09-07

### Added

- *(bitsandbytes)* add consuming enum alias normalization

## [0.4.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.3.2...bitsandbytes-v0.4.0) - 2026-09-07

### Added

- *(bitsandbytes)* [**breaking**] normalize builder enum aliases and prepare release
- *(bitsandbytes)* add bounded source windows

## [0.3.2](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.3.1...bitsandbytes-v0.3.2) - 2026-07-24

Headline (in the re-exported `bitsandbytes-macros` 0.3.2): every generated
accessor is now a `const fn`, and generated bitfield/bitflags structs are
`#[repr(transparent)]` over their backing integer. Upgrading from 0.3.1: a
hand-written `Bits` field type now needs `bnb::impl_bits!` (see the `Bits`
docs), a `#[view]` closure calling non-`const` helpers needs `dynamic` (or
`const` helpers), and a primitive `type` alias as a field type must become
the bare primitive name.

### Added

- *(bnb)* add impl_bits! — one-definition Bits impls for custom field types
- *(bnb)* add the inherent const conversion pair to UInt

### Other

- *(bnb)* cover every raw-type resolution rule and codegen arm of #[view]
- *(bnb)* record the stopgap status of the const dispatch
- *(bnb)* pin the alias failure mode, the view const assertion, impl_bits!
- *(bnb)* record the const-dispatch and repr(transparent) decisions
- *(bnb)* prove the generated accessors in const contexts

## [0.3.1](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.3.0...bitsandbytes-v0.3.1) - 2026-07-23

### Added

- *(bnb)* add contextual #[view] fields to #[bitfield]
- *(bnb)* add read-side #[br(calc = <expr>)]

## [0.3.0](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.2.1...bitsandbytes-v0.3.0) - 2026-07-06

### Fixed

- *(bitstream)* honor bit order in StreamBitReader ([#21](https://github.com/RawSocketLabs/rsl/pull/21))

### Other

- *(bnb)* add the migration guide + a whole-message perf baseline ([#31](https://github.com/RawSocketLabs/rsl/pull/31))
- *(bnb)* polish the guide + macro reference for 1.0 ([#30](https://github.com/RawSocketLabs/rsl/pull/30))
- *(bnb)* [**breaking**] seal Source/Sink/SeekSource/BitAmount ([#27](https://github.com/RawSocketLabs/rsl/pull/27))
- *(bnb)* [**breaking**] harden the public surface for the freeze ([#26](https://github.com/RawSocketLabs/rsl/pull/26))
- *(bnb)* [**breaking**] pre-freeze naming fixes (Error, codec, raw, with_cap) ([#25](https://github.com/RawSocketLabs/rsl/pull/25))
- *(bnb)* [**breaking**] unify the byte/bit-order vocabulary across all macros ([#23](https://github.com/RawSocketLabs/rsl/pull/23))
- *(bnb)* [**breaking**] cut the carried encode_mode mechanism ([#22](https://github.com/RawSocketLabs/rsl/pull/22))

## [0.2.1](https://github.com/RawSocketLabs/rsl/compare/bitsandbytes-v0.2.0...bitsandbytes-v0.2.1) - 2026-07-05

### Other

- update Cargo.toml dependencies

## [0.2.0](https://github.com/RawSocketLabs/bitsandbytes/compare/v0.1.0...v0.2.0) - 2026-06-22

### Added

- *(bin)* re-runnable validate() / is_valid() methods ([#21](https://github.com/RawSocketLabs/bitsandbytes/pull/21))
- *(bin)* [**breaking**] carry encode mode on the value; encode(w) follows it ([#20](https://github.com/RawSocketLabs/bitsandbytes/pull/20))
- *(bin)* canonical helpers — to_canonical / canonical_diff / is_canonical ([#16](https://github.com/RawSocketLabs/bitsandbytes/pull/16))
- *(bitfield)* custom Debug that decomposes the logical fields ([#15](https://github.com/RawSocketLabs/bitsandbytes/pull/15))

### Other

- *(examples)* add framed — bytes-feature framing + streaming ([#24](https://github.com/RawSocketLabs/bitsandbytes/pull/24))
- *(bin)* [**breaking**] cut inherent encode_into / canonical_encode_into ([#23](https://github.com/RawSocketLabs/bitsandbytes/pull/23))
- align design/roadmap/guide with the encode/validate/construct model ([#22](https://github.com/RawSocketLabs/bitsandbytes/pull/22))
- clarify the verbatim vs canonical encode model ([#19](https://github.com/RawSocketLabs/bitsandbytes/pull/19))
- *(examples)* add telemetry — synthetic stacking showcase ([#17](https://github.com/RawSocketLabs/bitsandbytes/pull/17))
- [**breaking**] encode(w, mode: EncodeMode) — fold canonical into BitEncode ([#18](https://github.com/RawSocketLabs/bitsandbytes/pull/18))
- *(examples)* add ipv4 — real IPv4 header parser ([#12](https://github.com/RawSocketLabs/bitsandbytes/pull/12))
- *(roadmap)* capture encode-model + #[default] open decisions ([#13](https://github.com/RawSocketLabs/bitsandbytes/pull/13))
- [**breaking**] split encode into verbatim (to_bytes) + canonical (to_canonical_bytes) ([#14](https://github.com/RawSocketLabs/bitsandbytes/pull/14))
- reorganize repo layout, declutter the root ([#11](https://github.com/RawSocketLabs/bitsandbytes/pull/11))
- add CONTRIBUTING.md ([#10](https://github.com/RawSocketLabs/bitsandbytes/pull/10))
- add SECURITY.md (threat model + dual-use scope + reporting) ([#9](https://github.com/RawSocketLabs/bitsandbytes/pull/9))
- *(bnb)* boundary stress for hostile count + the endian×bit-order matrix ([#8](https://github.com/RawSocketLabs/bitsandbytes/pull/8))
- *(semver-checks)* block SemVer breakage vs the last release tag ([#7](https://github.com/RawSocketLabs/bitsandbytes/pull/7))
- *(public-api)* add cargo-public-api snapshot + drift gate ([#6](https://github.com/RawSocketLabs/bitsandbytes/pull/6))
- *(lints)* forbid unsafe_code workspace-wide (zero-unsafe guarantee) ([#5](https://github.com/RawSocketLabs/bitsandbytes/pull/5))
- *(bnb)* add cargo-fuzz target for the decode path ([#4](https://github.com/RawSocketLabs/bitsandbytes/pull/4))
- *(bnb-macros)* rename codec source/sink params to __bnb_r/__bnb_w ([#1](https://github.com/RawSocketLabs/bitsandbytes/pull/1))

## [0.1.0] - 2026-06-19

Initial baseline of the `bitsandbytes` runtime crate.
