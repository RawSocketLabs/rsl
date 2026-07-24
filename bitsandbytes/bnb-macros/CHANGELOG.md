# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries below `0.1.0` are generated from Conventional Commits by release-plz.

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
