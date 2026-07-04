# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries below `0.1.0` are generated from Conventional Commits by release-plz.

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
