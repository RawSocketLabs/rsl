# SHA-256 vector provenance

Published test material belongs here only when its provenance is recorded. For each imported or
transcribed vector, record:

- evidence class: **published vector**, **standard-derived expectation**, **regression case**, or
  **differential result**;
- specification or vector-suite title and revision;
- section, example, or case identifier;
- authoritative retrieval location;
- date on which the authoritative source was checked;
- original representation and any conversion into a Rust fixture;
- a checksum when the original vector suite is stored as a file.

Do not silently copy values from another library's tests. That can reproduce the same mistake in
both the implementation and its supposed validation data. Do not describe a value calculated by
applying a published equation as though the publication itself supplied that value.

## NIST intermediate-value examples

- **Evidence class:** published vector.
- **Title:** NIST, *Secure Hash Algorithm — Message Digest Length = 256*.
- **Cases used:** “One Block Message Sample” (`"abc"`) and “Two Block Message Sample”
  (`"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"`).
- **Authoritative index:**
  <https://csrc.nist.gov/projects/cryptographic-standards-and-guidelines/example-values>.
- **Authoritative document:**
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/SHA256.pdf>.
- **Checked:** 2026-08-27.
- **Conversion:** the PDF's ASCII input messages are represented as Rust UTF-8 string literals;
  spaces between printed digest words were removed and hexadecimal letters were lowercased.

The public known-answer cases are in `tests/sha256/known_answers.rs`. Private schedule and
compression tests also use intermediate values explicitly printed by this document. Expectations
computed locally from a published rule are labeled **standard-derived**, not published.

## NIST CAVP byte-oriented test vectors

- **Evidence class:** published vector.
- **Suite:** NIST Cryptographic Algorithm Validation Program, *Secure Hashing* byte-oriented test
  vectors, CAVS 11.0.
- **Archive:** `shabytetestvectors.zip`.
- **Authoritative page:**
  <https://csrc.nist.gov/projects/cryptographic-algorithm-validation-program/secure-hashing>.
- **Authoritative archive:**
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Algorithm-Validation-Program/documents/shs/shabytetestvectors.zip>.
- **Checked:** 2026-08-27.
- **Downloaded archive SHA-256:**
  `929ef80b7b3418aca026643f6f248815913b60e01741a44bba9e118067f4c9b8`.
- **File used:** `shabytetestvectors/SHA256ShortMsg.rsp`, generated 2011-03-15.
- **Cases used:** `Len = 0`, `8`, `440`, `448`, `504`, and `512` bits.
- **Conversion:** each nonempty `Msg` hexadecimal string is decoded into exactly `Len / 8` Rust
  bytes; `MD` is decoded into exactly 32 bytes. For `Len = 0`, the response file's `Msg = 00` is
  a formatting placeholder and becomes an empty Rust array.

The selected cases live in `tests/sha256/boundaries.rs`. The archive is not copied into this
repository; the checksum and exact response-file case identifiers make the transcription
repeatable while keeping the test fixture small.

## Differential implementation

- **Evidence class:** differential result.
- **Implementation:** RustCrypto `sha2` crate, version `0.11.0`, used with default features
  disabled.
- **Upstream:** <https://github.com/RustCrypto/hashes/tree/master/sha2>.
- **Role:** development-only oracle over deterministic message lengths and fragmentations.
- **Checked:** 2026-08-27.

Differential agreement is supplementary evidence. It is not treated as a published NIST vector,
and production `rsl-crypto` code has no dependency on RustCrypto.
