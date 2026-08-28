# Cryptographic standards and implementation coverage

This file is the traceability ledger for `rsl-crypto`. It answers four questions for every
standards-based implementation:

1. Which exact publication and revision controls the implementation?
2. Which section, equation, table, or test case is represented by a piece of code?
3. How is the standard's notation represented in Rust?
4. Is that portion implemented and tested, or is it only planned?

An entry in this ledger is not a security or conformance claim. It documents implementation
intent and evidence so that reviewers can compare the code with the controlling publication.

## SHA-256 source baseline

The current SHA-256 work uses one controlling publication:

- **Publication:** NIST FIPS PUB 180-4, *Secure Hash Standard (SHS)*
- **Revision:** August 2015
- **Publication record:** [NIST CSRC: FIPS 180-4](https://csrc.nist.gov/pubs/fips/180-4/upd1/final)
- **Persistent identifier:** [doi:10.6028/NIST.FIPS.180-4](https://doi.org/10.6028/NIST.FIPS.180-4)
- **Document used for section and equation references:**
  [official NIST PDF](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf)
- **Baseline last checked:** 2026-08-27
- **Supersession state when checked:** final and not withdrawn. NIST's 2023 planning note says a
  revision is intended, but no replacement publication is yet listed.

The publication record is checked in addition to retaining the DOI because NIST uses that record
to publish revision and supersession notices. Adopting a newer revision requires a deliberate
ledger update and a review of every affected implementation row; a newer document must not be
silently treated as equivalent.

Supplementary validation material:

- **Publication:** NIST, *Secure Hash Algorithm — Message Digest Length = 256*
- **Index:** [NIST examples with intermediate values](https://csrc.nist.gov/projects/cryptographic-standards-and-guidelines/example-values)
- **Document:** [official SHA-256 example PDF](https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/SHA256.pdf)
- **Material currently used:** the one-block `abc` sample's block words, round states, final hash
  words, and digest; and the two-block sample's message and final digest
- **Source last checked:** 2026-08-27

This example is validation evidence, not the controlling algorithm definition. It does not
publish expanded words `W_16` through `W_63`, so this repository labels those expectations as
standard-derived.

Boundary validation material:

- **Suite:** NIST CAVP, *Secure Hashing* byte-oriented test vectors, CAVS 11.0
- **Source page:** [NIST CAVP Secure Hashing](https://csrc.nist.gov/projects/cryptographic-algorithm-validation-program/secure-hashing)
- **Archive:** [official byte-oriented vectors](https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Algorithm-Validation-Program/documents/shs/shabytetestvectors.zip)
- **File and cases:** `SHA256ShortMsg.rsp`; lengths 0, 8, 440, 448, 504, and 512 bits
- **Source last checked:** 2026-08-27

The exact archive checksum and fixture conversion are recorded in
`tests/vectors/sha256/README.md`. NIST describes these vectors as material for informal
verification; passing them is not a CAVP or CMVP validation claim.

## SHA-256 notation mapping

The reference implementation keeps the specification's operations visible instead of fusing or
rearranging them:

| FIPS 180-4 notation | Rust representation | Meaning |
| --- | --- | --- |
| 32-bit word | `u32` | The word size specified for SHA-256. |
| 512-bit message block | `[u8; 64]` | Exactly 64 octets; partial input belongs to the state layer. |
| `ROTR^n(x)` | `x.rotate_right(n)` | Rotation of a 32-bit word as defined in section 3.2. |
| `SHR^n(x)` | `x >> n` | Logical right shift; `x` is unsigned, so zero bits enter from the left. |
| `x AND y` | `x & y` | Bitwise conjunction. |
| `x XOR y` | `x ^ y` | Bitwise exclusive-or. |
| `NOT x` | `!x` | Bitwise complement of all 32 bits. |
| addition modulo `2^32` | `wrapping_add` | Overflow is required algorithm behavior, not an error. |
| big-endian word parsing | `u32::from_be_bytes` | The first input octet becomes the most-significant byte. |

These choices are part of the readable reference path. An optimized implementation may exist
beside it later, but must retain independent tests against this path and the same standard.

## SHA-256 coverage

Status meanings:

- **Implemented and tested:** executable code exists and focused tests exercise the cited rule.
  Evidence may be private white-box tests, public published vectors, or both, as identified in the
  row. This does not imply production readiness or formal validation.
- **Partially implemented:** only the named subset exists; the omitted work is stated explicitly.
- **Planned:** the module boundary and intended source are documented, but production code does
  not exist yet.

| FIPS 180-4 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §3.1, *Bits, Bytes, and Words* | Treat groups of four bytes as big-endian 32-bit words. | `schedule::parse_block`; sequential-byte and pre-padded `abc` parsing tests. | Implemented and locally tested. |
| §3.2, *Operations on Words* | Define bitwise operations, rotations, shifts, and addition modulo `2^32`. | `functions` uses explicit bitwise operations and `rotate_right`; `schedule::expand_word` uses `wrapping_add`; focused operator and overflow tests. | Implemented for the operations currently needed. |
| §4.1.2, equations 4.2–4.7 | Define `Ch`, `Maj`, uppercase sigma zero/one, and lowercase sigma zero/one. | The six named functions in `functions.rs`; truth-table and bit-movement tests. | Implemented and locally tested. |
| §4.2.2 | Supply `K_0` through `K_63` for the SHA-256 rounds. | `constants::ROUND_CONSTANTS`; all 64 values are compared in published order. | Implemented and locally tested. |
| §5.1.1 | Pad SHA-256 messages with a `1` bit, zero bits, and the 64-bit message length. | `state::build_final_blocks`; white-box 0-, 55-, and 56-byte layout tests plus public CAVP vectors at 0, 55, 56, 63, and 64 bytes. | Implemented and tested. |
| §5.2.1 | Parse each 512-bit message block into sixteen 32-bit words. | `schedule::parse_block`. | Implemented and locally tested. |
| §5.3.3 | Initialize SHA-256 with eight specified 32-bit hash words. | `constants::INITIAL_HASH_VALUE`; all eight words are compared in published order. | Implemented and locally tested. |
| §6.2.1 | Perform preprocessing before hash computation. | `state::Sha256` tracks total length, buffers at most 63 bytes, compresses complete blocks, and constructs final padding. Public fragmentation and boundary tests cover composition. | Implemented and tested for byte-aligned input. |
| §6.2.2, message schedule | Expand `W_16` through `W_63` using the specified four-term recurrence. | `schedule::expand_word` exposes one recurrence step; `schedule::build_schedule` constructs all 64 words. Argument-position, overflow, parsed-word preservation, and complete `abc` schedule tests cover the layer. | Implemented and locally tested. |
| §6.2.2, steps 1–3 | Initialize working variables, execute 64 rounds using `T_1` and `T_2`, then add the result into the chaining value. | `compression::WorkingVariables` implements step 1. `calculate_temporaries`, `advance_working_variables`, and `perform_round` expose one step 2 round; `run_rounds` applies all 64 `K_t` and `W_t` values; `feed_forward` implements step 3; and `compress_block` composes schedule construction with all three steps. Derived temporary/overflow tests and NIST-published `t=0`, `t=63`, output-word, and complete-block evidence cover the boundaries. | Implemented and locally tested for one complete block. |
| §6.2.2, final paragraph | Concatenate the final eight hash words as a 256-bit message digest. | `state::serialize_digest` and public `Sha256Digest`; focused word-order/endianness test and published public digest vectors. | Implemented and tested. |

The public `Sha256` implementation additionally has differential tests against RustCrypto `sha2`
0.11.0 over deterministic messages spanning padding and multi-block boundaries. RustCrypto is a
development-only dependency and is supplementary evidence, not an implementation dependency or
a standards authority.

The `abc` values for `W_16` through `W_63` are **derived test expectations**: they are calculated
by applying the recurrence in section 6.2.2 to the published pre-padded block. Neither FIPS 180-4
nor NIST's supplementary SHA-256 example publishes those expanded words as a known-answer vector.
Tests and reviews must preserve that distinction.

## SHA-512 source baseline

SHA-512 uses the same controlling NIST publication as SHA-256:

- **Publication:** NIST FIPS PUB 180-4, *Secure Hash Standard (SHS)*, August 2015.
- **Publication record:** [NIST CSRC: FIPS 180-4](https://csrc.nist.gov/pubs/fips/180-4/upd1/final).
- **Persistent identifier:** [doi:10.6028/NIST.FIPS.180-4](https://doi.org/10.6028/NIST.FIPS.180-4).
- **Document:** [official NIST PDF](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf).
- **Baseline last checked:** 2026-08-27.
- **Supersession state:** final and not withdrawn; the SHA-256 baseline above records NIST's
  revision-planning notice.

Published evidence uses FIPS 180-4's `abc` and 112-byte SHA-512 examples. Exact message/digest
conversion is recorded in `tests/vectors/sha512/README.md`. RustCrypto `sha2` 0.11.0 supplies a
development-only differential oracle over padding and block boundaries.

## SHA-512 notation mapping and coverage

| FIPS 180-4 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §§3.1–3.2 | Use big-endian 64-bit words, bitwise operations, rotations, shifts, and addition modulo `2^64`. | `sha512::functions`, `schedule::parse_block`, `wrapping_add`; truth-table, bit-position, byte-order, and recurrence tests. | Implemented and tested. |
| §4.1.3, equations 4.8–4.13 | Define SHA-512 `Ch`, `Maj`, and four sigma functions with the SHA-512 rotation distances. | Six separately named functions and focused operator tests. | Implemented and tested. |
| §4.2.3 | Supply `K_0..K_79`. | `constants::ROUND_CONSTANTS`; boundary/order assertions plus complete published digest evidence. | Implemented and tested. |
| §5.1.2 | Append the marker, zeroes, and a 128-bit big-endian original message length. | `state::final_blocks`; 111/112-byte boundary tests and differential cases around both padding thresholds. | Implemented for byte-aligned input. |
| §5.2.2 | Parse each 1024-bit block as sixteen 64-bit words. | `schedule::parse_block`; sequential-byte test. | Implemented and tested. |
| §5.3.5 | Initialize eight published 64-bit chaining words. | `constants::INITIAL_HASH_VALUE`; exact first/final word assertions and complete digests. | Implemented and tested. |
| §6.4.2 | Expand eighty schedule words, execute eighty `T1`/`T2` rounds, and feed forward. | `schedule::build_schedule`, `compression::perform_round`, and `compress_block`; first-round named-transition test, published digests, and differential evidence. | Implemented and tested. |
| §6.4 final output | Concatenate `H_0..H_7` as 512 big-endian bits. | `Sha512Digest`; two published complete digests and exact-size public API. | Implemented and tested. |

SHA-512 is independently public because it is a useful digest and the exact `H` required by
Ed25519. Ed25519 calls this implementation; it does not hide a production dependency on the
differential oracle.

## HMAC-SHA-256 source baseline

The HMAC-SHA-256 work uses the current final NIST construction publication as its controlling
source and cross-checks it against the original construction RFC:

- **Controlling publication:** NIST FIPS PUB 198-1, *The Keyed-Hash Message Authentication Code
  (HMAC)*, July 2008.
- **Publication record:** [NIST CSRC: FIPS 198-1](https://csrc.nist.gov/pubs/fips/198-1/final).
- **Persistent identifier:** [doi:10.6028/NIST.FIPS.198-1](https://doi.org/10.6028/NIST.FIPS.198-1).
- **Document:** [official NIST PDF](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.198-1.pdf).
- **Construction cross-check:** [RFC 2104, §2](https://www.rfc-editor.org/rfc/rfc2104.html),
  February 1997, [doi:10.17487/RFC2104](https://doi.org/10.17487/RFC2104).
- **Baseline last checked:** 2026-08-27.
- **Supersession state when checked:** FIPS 198-1 remains final and is not listed as withdrawn.
  NIST has proposed withdrawing it when NIST SP 800-224 is finalized; SP 800-224 remains an
  Initial Public Draft and is not silently used as the controlling publication.

Published validation material:

- **Publication:** RFC 4231, *Identifiers and Test Vectors for HMAC-SHA-224, HMAC-SHA-256,
  HMAC-SHA-384, and HMAC-SHA-512*, December 2005.
- **Document:** [RFC Editor HTML](https://www.rfc-editor.org/rfc/rfc4231.html).
- **Material used:** §4.2–§4.8, Test Cases 1–7. Test Cases 1–4, 6, and 7 supply full
  HMAC-SHA-256 tags; Test Case 5 supplies only a 128-bit prefix.
- **Source last checked:** 2026-08-27.

Exact fixture conversion and the intentionally truncated status of Test Case 5 are recorded in
`tests/vectors/hmac-sha256/README.md`.

## HMAC-SHA-256 notation mapping and coverage

For SHA-256, FIPS 198-1's `B` is 64 bytes and `L` is 32 bytes. `K0` is a distinct secret
64-byte internal value; `ipad` and `opad` are full 64-byte blocks containing repeated `0x36` and
`0x5c`, respectively. `||` is implemented by sequential digest updates, and `XOR` is a visible
byte-by-byte exclusive-or operation.

| FIPS 198-1 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §2.3 | Bind `B`, `L`, `ipad`, and `opad` to the chosen hash. | `key::KEY_BLOCK_LEN`, `state::TAG_LEN`, `key::INNER_PAD_BYTE`, and `key::OUTER_PAD_BYTE`; exact-size types and focused pad tests. | Implemented and tested. |
| §3; §4, Table 1, steps 1–3 | Use `K` directly at 64 bytes, hash keys longer than 64 bytes, and zero-pad shorter keys to form `K0`. | `key::NormalizedKey::from_key`; short-, exact-, and long-key layer tests plus RFC 4231 long-key cases. | Implemented and tested. |
| §4, equation and Table 1, steps 4–9 | Compute the inner digest and then the outer digest using `K0 XOR ipad` and `K0 XOR opad`. | `key::NormalizedKey::into_padded_blocks` and `state::HmacSha256`; all seven RFC 4231 cases, fragmentation tests, and differential comparison. | Implemented and tested. |
| §5 | If truncation is offered, return the leftmost requested bits subject to an explicit policy. | The first API exposes only a full tag. RFC 4231 Test Case 5 confirms that the published 128-bit value is the leftmost prefix without presenting it as a complete tag. | Deliberately deferred at the API boundary. |
| §4 plus verification contract | Compare a supplied full tag without value-dependent early exit. | Distinct `HmacSha256Tag`; exact, wrong-byte, short, and long public verification tests. The XOR/OR comparison source has not received compiler-level constant-time analysis. | Implemented with an explicit assurance limitation. |

The public implementation additionally has differential tests against RustCrypto `hmac` 0.13.0
and `sha2` 0.11.0 over deterministic short, exact-block, and long keys and message-boundary
lengths. Both crates are development-only dependencies and are not standards authorities.

This primitive layer owns HMAC computation and full-tag verification. TLS and SSH code will
continue to own protocol-selected truncation rules, key derivation, packet/record framing, and
activation state.

## HKDF-SHA-256 source baseline

- **Controlling publication:** RFC 5869, *HMAC-based Extract-and-Expand Key Derivation Function
  (HKDF)*, May 2010.
- **Status:** Informational IETF consensus publication.
- **Publication record:** [RFC Editor: RFC 5869](https://www.rfc-editor.org/info/rfc5869/).
- **Persistent identifier:** [doi:10.17487/RFC5869](https://doi.org/10.17487/RFC5869).
- **Document:** [RFC Editor HTML](https://www.rfc-editor.org/rfc/rfc5869.html).
- **Baseline last checked:** 2026-08-27.
- **Errata state:** RFC Editor Errata ID 5161 is reported as editorial, not verified. It clarifies
  §2.3's prose for the single-octet Expand counter. The implementation follows the section's
  displayed `0x01`, `0x02`, … recurrence and `N <= 255` bound, so no counter wraps.

NIST SP 800-56C Rev. 2 specifies extraction-then-expansion methods for NIST key-establishment
schemes, but it is not the controlling source for this generic RFC 5869 implementation. Its
publication record was checked on 2026-08-27 and notes that NIST decided in January 2026 to revise
it. Protocols requiring SP 800-56C conformance must add that profile in their own standards
ledger rather than silently treating generic HKDF as the complete scheme.

Published validation material is RFC 5869 Appendix A.1–A.3. All three SHA-256 cases publish the
input keying material (`IKM`), optional `salt`, context `info`, output length `L`, extracted
pseudorandom key (`PRK`), and output keying material (`OKM`). Exact conversion rules are recorded
in `tests/vectors/hkdf-sha256/README.md`.

## HKDF-SHA-256 notation mapping and coverage

| RFC 5869 location | Requirement represented | Intended code and evidence | Status |
| --- | --- | --- | --- |
| §2.2 | Compute `PRK = HMAC-Hash(salt, IKM)` and substitute `HashLen` zero octets when salt is absent. | `kdf::hkdf::sha256::extract` and distinct zeroizing `HkdfSha256Prk`; all three Appendix A SHA-256 PRK values, including explicit-empty and absent salt. | Implemented and tested. |
| §2.3 | Compute `T(1)` through `T(N)` from the prior block, `info`, and a one-octet counter. | `HkdfSha256Prk::expand`; focused first/subsequent-block recurrence test, all three Appendix A SHA-256 OKMs, and differential comparison. | Implemented and tested. |
| §2.3 | Require `L <= 255 * HashLen` and return exactly the first `L` octets. | Length validation precedes output mutation; public tests cover zero, partial/multiple blocks, exact 8,160-byte success, and unchanged 8,161-byte rejection. | Implemented and tested. |
| §3 | Keep salt, input keying material, and context roles distinct. | Separate parameters and secret-bearing PRK type; no implicit serialization or protocol-specific context construction. | Implemented in the primitive API. |

HKDF owns generic byte-oriented extraction and expansion. TLS labels, SSH key material layout, and
other protocol-specific `info` encodings remain in their protocol repositories.

The public implementation has differential tests against RustCrypto `hkdf` 0.13.0 with `sha2`
0.11.0 across absent and present salts, varied input/context lengths, block boundaries, and the
exact 8,160-byte maximum. Both are development-only dependencies and are supplementary evidence.

## AES-128 source baseline

- **Controlling publication:** NIST FIPS 197-upd1, *Advanced Encryption Standard (AES)*.
- **Revision:** Published November 26, 2001; updated May 9, 2023.
- **Publication record:** [NIST CSRC: FIPS 197](https://csrc.nist.gov/pubs/fips/197/final).
- **Persistent identifier:**
  [doi:10.6028/NIST.FIPS.197-upd1](https://doi.org/10.6028/NIST.FIPS.197-upd1).
- **Document:**
  [official NIST PDF](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf).
- **Baseline last checked:** 2026-08-27.
- **Supersession state when checked:** final and not withdrawn. This May 2023 publication
  supersedes the November 2001 edition. NIST states that the update made no technical changes to
  the AES algorithm, while improving terminology, formatting, and key-schedule diagrams.

Published validation material:

- **Controlling-standard examples:** FIPS 197-upd1 Appendix A.1 publishes the AES-128 key
  expansion, and Appendix B publishes a step-by-step AES-128 cipher example.
- **Supplementary NIST example index:**
  [NIST examples with intermediate values](https://csrc.nist.gov/projects/cryptographic-standards-and-guidelines/example-values).
- **Supplementary AES-128 document:**
  [official AES_Core128 PDF](https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/AES_Core128.pdf).
- **Source last checked:** 2026-08-27.

Exact fixture provenance and conversion policy are recorded in
`tests/vectors/aes-128/README.md`. The supplementary document is evidence, not the controlling
algorithm definition, and passing its examples will not constitute NIST validation.

## AES-128 notation mapping and coverage

The readable state uses `rows[r][c]` for the standard's `s[r, c]`. Public key and block values are
distinct owned types even though both contain exactly sixteen bytes. At the private boundary,
explicit `row + 4 * column` indexing maps block arrays to the standard's column-major state without
making later state equations read backwards. Internal state, blocks, key owners, round keys, and
the expanded schedule are non-`Copy` and zeroized on drop because they contain plaintext or
key-dependent material.

| FIPS 197-upd1 location | Requirement represented | Intended code and evidence | Status |
| --- | --- | --- | --- |
| §3.1 | Accept a 128-bit key for AES-128 and transform blocks of exactly 128 bits. | Public `Aes128Key` and `Aes128Block` enforce distinct exact-size owners; the private boundary retains `[u8; 16]` arrays. | Implemented and publicly tested. |
| §3.4, equations 3.6 and 3.7; Figure 1 | Map sequential input/output bytes to and from the four-row, four-column state using `s[r,c] = in[r + 4c]`. | `state::State::from_block` and `state::State::write_block`; published Appendix B row mapping plus a distinct-byte round-trip test. | Implemented and tested. |
| §3.5, equation 3.8 | Treat a block or round key as four words, where word index is state column and byte index is state row. | `key::RoundKey`; published Appendix B key split into all four words. | Implemented and tested for one round key. |
| §4.1; equation 4.2 | Add two `GF(2^8)` elements as polynomials over `GF(2)`. | `field::add`; NIST's published `{57} XOR {83} = {d4}` example. | Implemented and tested. |
| §4.2; equations 4.3–4.7 | Multiply bytes as polynomials modulo `x^8 + x^4 + x^3 + x + 1`, including `XTIMES`. | `field::xtimes` and fixed-eight-step `field::multiply`; all equation 4.6/4.7 examples, exhaustive multiply-by-two equivalence, and zero/one identities. | Implemented and tested. |
| §4.4; equations 4.10 and 4.11; §5.1.1, equation 5.2 zero case | Calculate each nonzero inverse as `b^254` and map zero to zero at the S-box boundary. | `field::multiplicative_inverse_or_zero`; fixed exponentiation chain and exhaustive defining-equation tests for all 255 nonzero bytes. | Implemented and tested. |
| §4.3, equations 4.8 and 4.9; §5.1.3, equations 5.6–5.8 | Multiply a four-byte word by the fixed forward AES matrix. | `transforms::mix_column`; published Appendix B round-1 first-column input and output. | Implemented and tested for the forward matrix. |
| §5.1.1, equations 5.2–5.4; Table 4 | Compose field inversion (with zero mapped to zero) and the affine bit transform into `SBOX()`. | `substitution::substitute_byte`; calculation-only production path checked against all 256 published Table 4 entries and the `{53}` → `{ed}` prose example. | Implemented and tested for individual bytes. |
| §5.3.2; Table 6; inverse derived from equations 5.3–5.4 | Calculate `INVSBOX()` without a secret-indexed production table. | `substitution::inverse_affine_transform` and `inverse_substitute_byte`; forward-affine cancellation, all 256 published Table 6 entries, and exhaustive forward/inverse cancellation. | Implemented and tested for individual bytes. |
| §5.1.1 | Apply `SBOX()` independently to all sixteen state bytes as `SUBBYTES()`. | `transforms::sub_bytes`; FIPS Appendix B round-1 start and after-SubBytes matrices. | Implemented and tested. |
| §5.1.2, equation 5.5 | Replace each `s[r,c]` with `s[r,(c+r) mod 4]` as `SHIFTROWS()`. | `transforms::shift_rows`; FIPS Appendix B round-1 after-SubBytes and after-ShiftRows matrices. | Implemented and tested. |
| §5.1.3, equations 5.6–5.8 | Apply the fixed forward matrix independently to all four columns as `MIXCOLUMNS()`. | `transforms::mix_columns`; FIPS Appendix B round-1 after-ShiftRows and after-MixColumns matrices. | Implemented and tested. |
| §5.1.4, equation 5.9; §5.3.4 | XOR four round-key words into corresponding state columns as `ADDROUNDKEY()`; the same operation is its own inverse. | `transforms::add_round_key` and distinct zeroizing `key::RoundKey`; FIPS Appendix B initial addition plus a twice-applied inverse-property test. | Implemented and tested. |
| §5.3.1, equation 5.12 | Shift state row `r` right by `r` positions as `INVSHIFTROWS()`. | `transforms::inverse_shift_rows`; NIST AES_Core128 first decryption boundary and forward/inverse cancellation. | Implemented and tested. |
| §5.3.2 | Apply `INVSBOX()` independently to every state byte as `INVSUBBYTES()`. | `transforms::inverse_sub_bytes`; NIST AES_Core128 first decryption boundary and forward/inverse cancellation. | Implemented and tested. |
| §5.3.3, equations 5.13–5.15 | Apply the fixed inverse field matrix independently to every state column. | `transforms::inverse_mix_column` and `inverse_mix_columns`; NIST AES_Core128 round-9 boundary and forward/inverse cancellation. | Implemented and tested. |
| §5.2, Table 5 | Supply the ten `Rcon[j]` words consumed by AES-128 expansion. | `key_schedule::ROUND_CONSTANTS`; all ten literal words compared in published order. | Implemented and tested. |
| §5.2, equations 5.10 and 5.11 | Define `ROTWORD()` and `SUBWORD()` without endian reinterpretation or a production S-box table. | `key_schedule::rotate_word` and `key_schedule::substitute_word`; Appendix A.1 intermediates at `i = 4` and `i = 8`. | Implemented and tested. |
| §5.2, Algorithm 2, Figure 6 | Expand one 128-bit key into 44 words and eleven round keys. | Non-`Clone`, zeroizing `key_schedule::KeySchedule`; all 44 Appendix A.1 words and all eleven four-word groupings. | Implemented and tested for AES-128. |
| §5.1, Algorithm 1; Table 3 | Apply initial key addition, nine full AES-128 rounds, and the final round without `MIXCOLUMNS`. | `forward::encrypt_block` under public `Aes128::encrypt_block`; FIPS Appendix B and all four NIST `AES_Core128.pdf` encryption blocks. | Implemented and publicly tested. |
| §5.3, Algorithm 3; Table 3 | Apply final key addition, nine complete inverse rounds in reverse key order, and the last inverse round without `INVMIXCOLUMNS()`. | `inverse::decrypt_block` under public `Aes128::decrypt_block`; FIPS Appendix B and all four NIST AES_Core128 decryption blocks plus varied round trips. | Implemented and publicly tested. |

The public implementation additionally has differential tests against RustCrypto `aes` 0.9.2 over
192 deterministic key/block pairs in both directions. RustCrypto is a development-only dependency
and supplementary evidence, not an implementation dependency or standards authority.

The reference implementation calculates secret-dependent substitutions rather than indexing a
lookup table with secret data. That policy keeps the straightforward path aligned with
`DESIGN.md`; it is not by itself a side-channel-resistance claim. The public API intentionally
exposes only the raw block permutation and clearly documents that it supplies no nonce, mode,
authentication, padding, framing, or arbitrary-length message encryption. Those properties come
from a separately specified construction such as the public AES-128-GCM layer below.

## GHASH source baseline

- **Controlling publication:** NIST SP 800-38D, *Recommendation for Block Cipher Modes of
  Operation: Galois/Counter Mode (GCM) and GMAC*.
- **Revision:** November 2007 final publication.
- **Publication record:** [NIST CSRC: SP 800-38D](https://csrc.nist.gov/pubs/sp/800/38/d/final).
- **Persistent identifier:**
  [doi:10.6028/NIST.SP.800-38D](https://doi.org/10.6028/NIST.SP.800-38D).
- **Document:**
  [official NIST PDF](https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf).
- **Baseline last checked:** 2026-08-27.
- **Revision state when checked:** the 2007 publication remains final. NIST's
  [June 1, 2026 second preliminary call for comments](https://csrc.nist.gov/pubs/sp/800/38/d/r1/2prd)
  for Revision 1 states that no draft document is available yet. It raises possible changes to GCM
  profiles and a separate 256-bit wide-GHASH for wGCM, but supplies no replacement definition for
  the current 128-bit GHASH.

Supplementary validation material:

- **Index:**
  [NIST examples with intermediate values](https://csrc.nist.gov/projects/cryptographic-standards-and-guidelines/example-values).
- **Document:**
  [official AES-GCM example PDF](https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/AES_GCM.pdf).
- **Material currently used:** GCM-AES128 Examples 1–5: the published AES-128 key, 96-bit IV,
  hash subkey `H`, pre-counter `J0`, AAD, plaintext, counter blocks, ciphertext, GHASH result `S`,
  and full tag values applicable to each layer. Example 2's individual first-block field product
  is a standard-derived test expectation because the document publishes its operands but not that
  intermediate result.
- **Source last checked:** 2026-08-27.

Exact fixture values, conversion rules, and evidence classification are recorded in
`tests/vectors/ghash/README.md` and `tests/vectors/gcm/README.md`.

## GHASH notation mapping and coverage

SP 800-38D's convention deserves special attention: in §6.3's displayed block
`x_0 x_1 ... x_127`, the leftmost bit `x_0` is the coefficient of `u^0`. Consequently, the field
identity is the block beginning with `0x80`, not the integer-looking block ending with `0x01`.
The implementation retains `[u8; 16]` block order and extracts displayed bits explicitly rather
than hiding this mapping in a native-endian integer conversion.

| SP 800-38D location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §5.3 | Keep the hash subkey and GCM intermediate values secret, and use GHASH only within GCM. | Private `aead::gcm::ghash` hierarchy; distinct non-`Clone`, zeroizing `HashSubkey`, accumulator, result, field elements, and temporaries; no standalone public GHASH API. | Implemented for the current GHASH layers. |
| §6.3, `R` definition; Algorithm 1 steps 1–2 | Read `x_0` through `x_127` in displayed order, initialize `Z_0 = 0^128` and `V_0 = Y`, and represent `R = 11100001 || 0^120`. | `field::displayed_bit`, literal `REDUCTION_FIRST_BYTE`, and focused bit-order/zero tests. | Implemented and locally tested. |
| §6.3, Algorithm 1 step 3 | For all 128 bits, conditionally add `V_i`, shift it right, and conditionally add `R` based on its rightmost bit. | `field::advance_changing_multiple` and `field::multiply`; both reduction paths, field identity, zero behavior, commutative published-operand case. | Implemented and tested with standard-derived evidence. |
| §6.3, Algorithm 1 step 4 | Return `Z_128` as `X • Y`. | `field::multiply`; NIST GCM-AES128 Example 2's published first ciphertext block and `H` produce an explicitly standard-derived individual product. | Implemented and tested privately. |
| §6.4, Algorithm 2 | Starting at `Y_0 = 0^128`, update `Y_i = (Y_(i-1) XOR X_i) • H` for every complete input block. | `state::Ghash`; first-iteration standard-derived evidence, NIST GCM-AES128 Example 2's published final `S` over four ciphertext blocks plus its length block, and varied differential comparison. | Implemented and tested privately for complete blocks. |

The field loop uses fixed iteration counts and masks rather than lookup tables or source-level
branches controlled by operand bits. This design has not received compiler-output or
platform-level constant-time analysis and is not a side-channel-resistance claim.

The complete-block recurrence additionally has differential tests against RustCrypto `ghash`
0.6.0 over 32 deterministic subkeys and sequences of one through eight blocks. RustCrypto is a
development-only dependency and supplementary evidence, not an implementation dependency or
standards authority. SP 800-38D Algorithms 4 and 5 still own partial-block zero padding, AAD and
ciphertext separation, bit-length encoding, and tag construction; those rules are not silently
folded into the Algorithm 2 primitive.

## AES-128-GCM coverage

The GCM composition uses the same SP 800-38D source baseline and supplementary `AES_GCM.pdf`
recorded above. Counter fixtures and conversion details are recorded separately in
`tests/vectors/gcm/README.md` because they validate GCM/GCTR composition rather than GHASH.

| SP 800-38D location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §5.1 | Use an approved 128-bit-block cipher, a key of at least 128 bits, and dedicate that key to GCM with the chosen cipher. | Public `Aes128GcmKey` is distinct from raw `Aes128Key`, owns exactly 128 bits, and is consumed into one non-`Clone` `Aes128Gcm` AES-128 schedule. `generate` fills the exact key through a caller-selected `RandomSource`, clearing its temporary on failure. | Implemented for AES-128; entropy-source approval and key lifecycle remain integration responsibilities. |
| §5.2.1.1; §5.2.2 | Support plaintext/ciphertext through `2^39 - 256` bits and AAD through `2^64 - 1` bits, with the same supported lengths for encryption and decryption; byte-oriented implementations may further restrict inputs. | `limits::validate_input_lengths` applies the exact byte-aligned bounds before either transform; boundary and next-byte rejection tests. Public `Aes128GcmNonce` deliberately fixes IVs at the recommended 96-bit length. | Implemented and tested for the documented byte-oriented profile. |
| §5.2.1.2 | Associate one supported tag length with the key and return ciphertext plus a tag. | Public `Aes128GcmTag` fixes `t = 128`; `Sealed<Aes128GcmTag>` keeps same-length ciphertext and detached tag distinct. No truncation input exists. | Implemented and publicly tested for `t = 128`. |
| §5.2.2 | Authenticated decryption returns plaintext only for the authentic IV/AAD/ciphertext/tag tuple; otherwise return `FAIL`. | Public `Aes128Gcm::open` returns owned plaintext only after `FullTag::verify`; all mismatches map to `CryptoError::AuthenticationFailed`. Public negative tests change AAD, nonce, every ciphertext byte, and every tag byte. | Implemented and publicly tested. |
| §6.1; §6.2 with `s = 32` | Preserve the leftmost 96 bits and replace the rightmost 32-bit integer with its value plus one modulo `2^32`. | Distinct private `counter::CounterBlock::increment`; explicit big-endian parse/serialization, published NIST `J0` → first counter block, carry, and wrap tests. | Implemented and tested privately. |
| §6.5, Algorithm 3 step 1 | Return empty output when the input string is empty. | `gctr::apply` performs no chunk iteration for an empty slice; focused empty-input test. | Implemented and tested for byte-aligned input. |
| §6.5, Algorithm 3 steps 2–5 | Partition input into complete blocks and at most one nonempty final partial block; use the initial counter as `CB_1`, then derive each later `CB_i` through `inc32`. | `gctr::apply` uses 16-byte mutable chunks and the distinct `CounterBlock`; published NIST Example 2 counter/ciphertext sequence. | Implemented and tested for byte-aligned input. |
| §6.5, Algorithm 3 steps 6–9 | Apply the forward block cipher to each counter, XOR complete blocks, use only the required leftmost cipher-output portion for the final partial block, and concatenate results. | Private in-place `gctr::apply` over public `Aes128`; NIST Example 2's four complete blocks, Example 5's 12-byte final block, and symmetry evidence. | Implemented and tested for byte-aligned input. |
| §7.1 Algorithm 4 step 1; §7.2 Algorithm 5 step 2 | Derive the secret hash subkey as `H = CIPH_K(0^128)`. | `setup::derive_hash_subkey` moves an encrypted zero `Aes128Block` into distinct `HashSubkey` storage; NIST GCM-AES128 Example 2's published `H`. | Implemented and tested for AES-128. |
| §7.1 Algorithm 4 step 2; §7.2 Algorithm 5 step 3, `len(IV) = 96` branch | Construct `J0 = IV || 0^31 || 1` directly. | Distinct `setup::GcmIv96` and `setup::PreCounterBlock`; published NIST Example 2 `IV`/`J0` plus distinct-byte position evidence. | Implemented and tested for exactly 96-bit IVs. |
| §7.1 Algorithm 4 step 2; §7.2 Algorithm 5 step 3, `len(IV) != 96` branch | Pad and GHASH a variable-length IV to derive `J0`. | No variable-length IV type or branch is exposed. | Deliberately deferred; unsupported by the initial profile. |
| §7.1 Algorithm 4 steps 4–5; §7.2 Algorithm 5 steps 5–6 | Independently zero-pad AAD and ciphertext to block boundaries, append their original lengths as two 64-bit bit counts, and calculate `S` with GHASH. | `authentication::calculate_s`; checked big-endian length block, borrowed complete blocks, zeroizing partial blocks; NIST Examples 2–5 published `S` values. | Implemented and tested for byte-aligned inputs whose bit lengths fit `u64`. |
| §7.1 Algorithm 4 step 6; §7.2 Algorithm 5 step 7 with `t = 128` | Calculate `MSB_t(GCTR_K(J0, S))` as the tag or candidate tag. | `tag::mask` transfers `S` through GCTR at unincremented `J0` into distinct zeroizing `FullTag`; NIST Examples 1–5 published `S`/tag pairs. | Implemented and tested for full 128-bit tags. |
| §7.1 Algorithm 4 | Compose supported-length validation, `H`/`J0` setup, GCTR encryption, authentication of AAD and ciphertext, and tag masking into authenticated encryption. | Private `seal::seal` keeps all six printed steps visible; public `Aes128Gcm::seal` returns a detached typed result. NIST Examples 1, 2, and 5 privately and Examples 1 and 5 publicly. | Implemented and publicly tested for AES-128, 96-bit nonces, and 128-bit tags. |
| §7.2 Algorithm 5 and paragraph after step 8 | Compose authenticated decryption, permitting tag verification to precede plaintext computation. | Private `open::open` authenticates before allocating/calling GCTR; public `Aes128Gcm::open` returns plaintext or one authentication error. NIST Examples 1 and 5 plus negative composition evidence. | Implemented and publicly tested for the same profile. |
| §§8.2.2–8.3; Appendix D | Permit an RBG-based IV construction within its invocation bounds, prevent key/IV reuse, and address replay at the consuming-system layer. | `Aes128GcmNonce::generate` fills one exact 96-bit nonce through a caller-selected `RandomSource`; documentation states that this does not track reuse or enforce invocation limits. No primitive-local sequence state or replay window is claimed; those belong to TLS/SSH protocol contexts. | Random-value convenience implemented; uniqueness, limits, sequence state, and replay policy remain explicit caller/protocol obligations. |

This counter is internal to one GCM invocation. It is distinct from a GCM nonce, a protocol record
sequence number, and SSH packet state; those lifetimes and advancement rules belong to their
owning construction or protocol layer.

GCTR accepts byte strings rather than arbitrary bit strings. That is the exact subset needed by
RSL's wire-oriented TLS and SSH use cases; a future bit-level caller would require a separately
specified and tested boundary. GCTR itself does not authenticate. Authenticated decryption must
verify the GCM tag before any transformed plaintext becomes caller-visible.

The authentication-input layer rejects a byte length that cannot be multiplied by eight into the
standard's 64-bit length field before starting GHASH. The complete GCM boundary additionally
enforces §5.2.1's smaller plaintext/ciphertext limit and byte-aligned AAD limit before encryption
or decryption.

The initial tag profile fixes `t = 128`; there is no raw truncation parameter. Other tag sizes
require an explicit policy and type, especially while NIST is revising SP 800-38D. This profile
covers the immediate full-tag TLS and SSH direction without implying support for every tag length
listed by the 2007 publication.

The public implementation additionally has differential tests against `RustCrypto` `aes-gcm`
0.11.1 over 32 deterministic keys and 96-bit nonces with independently varied AAD and payload
lengths. `RustCrypto` is a development-only dependency and supplementary evidence, not an
implementation dependency or standards authority.

## X25519 source baseline

- **Controlling publication:** RFC 7748, *Elliptic Curves for Security*.
- **Publication date and stream:** January 2016, IRTF Informational.
- **Publication record:** [RFC Editor: RFC 7748](https://www.rfc-editor.org/info/rfc7748/).
- **Persistent identifier:** [doi:10.17487/RFC7748](https://doi.org/10.17487/RFC7748).
- **Document:** [RFC Editor HTML](https://www.rfc-editor.org/rfc/rfc7748.html).
- **Baseline and errata last checked:** 2026-08-27.
- **Errata state:** four verified errata and one item held for document update. Verified Errata
  7625 clarifies the ladder's `swap ^= k_t` notation as XOR and directly affects how the source is
  explained, but not the published outputs. The other verified/held items concern the unused
  base-point `v` coordinate, wording around decoded decimal coordinates, Appendix A variable
  names, and Sage/Python syntax. Exact disposition is recorded in
  `tests/vectors/x25519/README.md`.

Published validation material comes from RFC 7748 §5.2's two direct X25519 vectors and iterative
checkpoints, plus §6.1's complete Alice/Bob key-agreement vector. Fixture conversion and the
routine-test treatment of the one-million-iteration checkpoint are recorded in
`tests/vectors/x25519/README.md`.

## X25519 notation mapping and coverage

The field layer uses five little-endian radix-`2^51` limbs for values modulo `p = 2^255 - 19`.
This is an implementation representation, not a change to the standard's integer arithmetic.
Every product coefficient uses `u128`; a carry beyond the 255th bit folds into limb zero after
multiplication by nineteen because `2^255 = 19 (mod p)`. Secret-dependent ladder coordinates are
non-`Clone`, non-formattable, and zeroized on drop.

| RFC 7748 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §4.1 | Use Curve25519 over `GF(2^255 - 19)` with Montgomery coefficient `A = 486662` and base u-coordinate 9. | `field::FieldElement`, `ladder::A24 = (A - 2) / 4`, and `api::BASE_COORDINATE`; field and published public-key tests. | Implemented and tested. |
| §5 coordinate decoding/encoding | Decode 32-byte coordinates little-endian, mask bit 255, accept non-canonical values as residues modulo `p`, and return canonical bytes with the high bit clear. | `FieldElement::from_bytes`/`to_bytes`; high-bit equality, `p -> 0`, `p + 9 -> 9`, second direct vector retaining final byte `0x93`. | Implemented and tested. |
| §5 scalar decoding | Clear scalar bits 0–2 and 255, set bit 254, and read the result little-endian. | `scalar::PreparedScalar`; exact prepared-byte and bit-position tests plus all published complete vectors. | Implemented and tested. |
| §5 ladder initialization and recurrence | Initialize the two projective pairs, process bits 254 through zero, preserve the named field equations, use `a24 = 121665`, perform the final swap, and return `x_2 * z_2^(p-2)`. | `ladder::scalar_multiply`, `FieldElement::invert`; both direct vectors, one/1,000 iterations, and §6.1 exchange. | Implemented and tested. |
| §5 `cswap`; Verified Errata 7625 | Use XOR for `swap` and implement conditional swaps without control-dependent branches or memory access. | `swap_control ^= scalar_bit` and `FieldElement::conditional_swap` with the RFC's `0 - swap` mask; focused zero/one controls. | Implemented at source level; compiler/platform timing not certified. |
| §5.1 | Execute the same field-operation sequence for every secret scalar and avoid secret-indexed memory. | Fixed 255-iteration ladder, fixed limb loops, masked swaps, and no lookup tables or division. | Implemented as a source policy; side-channel review remains incomplete. |
| §5.2 | Match direct and iterative X25519 test vectors. | Both direct triples and the one- and 1,000-iteration published checkpoints. The one-million value is recorded but not in routine tests. | Implemented and tested at the documented routine evidence level. |
| §6.1 | Derive public keys with coordinate 9 and equal shared secrets from each peer's private/public pair. | Public `X25519::public_key` and `X25519::agree`; complete published Alice/Bob public and shared values. | Implemented and publicly tested. |
| §6.1; §7 | Permit an all-zero shared result check to detect small-order input and avoid assuming contributory behavior. | `X25519::agree` ORs all result bytes, clears a rejected temporary, and returns `CryptoError::InvalidPublicKey`; zero and non-canonical `p` cases. | Implemented as a stricter public profile. |
| §6.1; §7 | Feed the result and required public context into a KDF; do not use public keys alone as identities or omit them from protocol context. | Shared secret requires explicit exposure. Rustdoc assigns KDF inputs, identity, transcript, and framing to TLS/SSH protocol code. | Explicit protocol obligation; not implemented in this primitive. |

The public implementation additionally has differential tests against RustCrypto
`x25519-dalek` 3.0.0 over 64 deterministic key pairs and 64 arbitrary coordinate encodings.
`x25519-dalek` is a development-only dependency and supplementary evidence, not an implementation
dependency or standards authority.

## Ed25519 source baseline

- **Controlling publication:** RFC 8032, *Edwards-Curve Digital Signature Algorithm (EdDSA)*.
- **Publication date and stream:** January 2017, IRTF Informational.
- **Publication record:** [RFC Editor: RFC 8032](https://www.rfc-editor.org/info/rfc8032/).
- **Document:** [RFC Editor HTML](https://www.rfc-editor.org/rfc/rfc8032.html).
- **Persistent identifier:** [doi:10.17487/RFC8032](https://doi.org/10.17487/RFC8032).
- **Errata record:** [RFC 8032 errata](https://errata.rfc-editor.org/search/?rfc_number=8032&presentation=records).
- **Baseline and errata last checked:** 2026-08-27.

The current errata record contains six verified, four held-for-document-update, and one rejected
record. The implementation directly accounts for verified Errata 5930 (missing exact-length
failure in the illustrative verifier), 5968 (`0 <= S < L`, including both endpoints), 6348
(point-multiplication notation), 8197 (the corrected external doubling-formula section), 5519
(`S` capitalization), and 6851 (IUF acronym). Held records are not silently treated as normative.
In particular, the strict verification policy is documented as an API hardening choice and tested
against an explicit strict oracle rather than being presented as a correction to RFC 8032.

Published validation material is RFC 8032 §7.1 tests 1–3, covering private seed, public key,
message, and complete signature. Conversion and oracle provenance are recorded in
`tests/vectors/ed25519/README.md`.

## Ed25519 notation mapping and coverage

| RFC 8032 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §3.1 and verified Errata 5968 | Encode scalars little-endian and accept canonical `S` exactly in `0..L-1`. | `scalar::from_canonical_bytes`; `L-1`/`L` focused tests and public non-canonical-`S` rejection. | Implemented and tested. |
| §5.1 parameters | Bind pure Ed25519 to SHA-512, `p=2^255-19`, `d=-121665/121666`, cofactor 8, subgroup order `L`, and the fixed basepoint. | `sha512`, `field` constants, `scalar::ORDER`, and `point::basepoint`; basepoint encoding and arithmetic tests. | Implemented and tested. |
| §5.1.1 | Perform inversion and the `(p-5)/8` square-root recovery in the field. | `FieldElement::invert`, `pow_p58`, and `square_root_ratio`; basepoint root and varied point-round-trip tests. | Implemented and tested. |
| §§5.1.2–5.1.3 | Encode canonical `y` plus the low bit of `x`; reject non-canonical `y`, missing roots, and negative zero. | `EdwardsPoint::compress`/`decompress`; canonical-boundary, negative-zero, and round-trip tests. | Implemented and tested. |
| §5.1.4 | Use complete extended-coordinate addition over `(X,Y,Z,T)`. | `EdwardsPoint::add` retains the RFC's `A..H` intermediates; scalar multiplication and RFC vectors compose it. | Implemented and tested. |
| §5.1.5 | Hash and prune the 32-byte seed, then derive `A=[s]B`. | `prepare_secret_scalar`, `verifying_key`; all three §7.1 public keys and differential cases. | Implemented and tested. |
| §5.1.6 | Derive deterministic `r`, encode `R`, derive challenge `k`, and compute `S=(r+k*s) mod L`. | `sign_bytes`, `hash_to_scalar`, and scalar modular arithmetic; all three §7.1 signatures and 32 differential signatures. | Implemented and tested for pure Ed25519. |
| §5.1.7 | Parse `R`, `S`, and `A`; hash `R||A||M`; verify a permitted group equation. | `verify_bytes` rejects malformed/non-canonical/small-order inputs and checks the sufficient uncofactored equation; published, changed-input, strict-boundary, generic-trait, and differential tests. | Implemented and tested with documented strict hardening. |
| §5.1 `dom2(F, C)`; §§5.1.6–5.1.7 domain input | Pure Ed25519 uses the empty string; Ed25519ctx prefixes `dom2(0, C)` with non-empty `C`; Ed25519ph prefixes `dom2(1, C)` and signs `PH(M) = SHA-512(M)`. | Private `api::dom2` and one shared signing/verification core; `Ed25519Context` enforces `1 <= len(C) <= 255`; distinct `sign_with_context`/`verify_with_context` and `sign_prehashed`/`verify_prehashed` methods rather than flags. Layout unit test; cross-variant rejection tests. | Implemented and tested. |
| §7.2 | Ed25519ctx vectors `foo`, `bar`, `foo2`, `foo3`. | All four public keys, signatures, and verifications. | Implemented and tested. |
| §7.3 | Ed25519ph vector `abc` with empty context. | Public key, signature, and verification; 32 differential Ed25519ph cases (with and without context) against `ed25519-dalek`'s prehashed path. | Implemented and tested. |
| §8.5 | Prehashing weakens collision resilience; pure Ed25519 is preferred. | Rustdoc states the preference; Ed25519ph is an explicit opt-in method. | Documented. |
| §8 | Address private-key secrecy, side channels, message semantics, and random seed generation. | Secret seed/scalars/points are non-`Clone` owners where public API permits, redacted, and zeroized; fixed scalar schedules and explicit `RandomSource` seed generation are visible. No compiler-level constant-time or audit claim. | Source-level measures implemented; production assurance incomplete. |

`ed25519-dalek` 3.0.0 is used only in development tests. Its public-key derivation, deterministic
signatures, and `verify_strict` behavior agree over 32 deterministic pure cases, and its
`sign_prehashed`/`verify_prehashed` path agrees over 32 Ed25519ph cases. It offers no Ed25519ctx
oracle; the four RFC vectors are the independent evidence for that variant. TLS certificate and
`CertificateVerify` encodings, SSH public-key blobs and exchange hashes, key identifiers,
transcript construction, and negotiation remain in their protocol repositories.

## P-256 source baseline

The NIST P-256 group is shared by ECDH and ECDSA and is implemented once under `src/curve/p256/`.

- **Curve parameters:** NIST SP 800-186, *Recommendations for Discrete Logarithm-based
  Cryptography: Elliptic Curve Domain Parameters*, February 2023, §3.2.1.3.
  [Publication record](https://csrc.nist.gov/pubs/sp/800/186/final);
  [doi:10.6028/NIST.SP.800-186](https://doi.org/10.6028/NIST.SP.800-186).
- **Point encoding:** SEC 1 v2.0, *Elliptic Curve Cryptography*, Certicom Research, May 2009,
  §2.3.3 (point to octet string), §2.3.4 (octet string to point), §2.3.5–§2.3.6 (field element
  and octet string conversion). [Document](https://www.secg.org/sec1-v2.pdf).
- **Group law:** J. Renes, C. Costello, L. Batina, *Complete addition formulas for prime order
  elliptic curves*, EUROCRYPT 2016, Algorithm 4 (`a = -3`).
  [IACR ePrint 2015/1060](https://eprint.iacr.org/2015/1060). This is a peer-reviewed formula
  source, not a standard; SP 800-186 defines the group but prescribes no formulas.
- **Baseline last checked:** 2026-08-28. SP 800-186 is final and supersedes the curve
  definitions formerly in FIPS 186-4 Appendix D. FIPS 186-4's fast-reduction appendix is not
  used; reduction is derived from the prime's form as documented in `arithmetic.rs`.

### P-256 notation mapping

| Publication notation | Rust representation | Meaning |
| --- | --- | --- |
| `p`, `n` | `arithmetic::Modulus` with `value` and `complement = 2^256 - m` | Four little-endian `u64` limbs; `u128` products and carries. |
| integer to octet string | `arithmetic::to_be_bytes` | 32 bytes, most significant first. |
| field element `x` | `field::FieldElement` | Residue below `p`; non-`Clone`, zeroizing. |
| scalar `d`, `r`, `s`, `e` | `scalar::Scalar` | Residue below `n`; non-`Clone`, zeroizing. |
| `(X : Y : Z)` | `point::ProjectivePoint` | Homogeneous projective coordinates; `O = (0 : 1 : 0)`. |
| `(x, y)` | `point::AffinePoint` | Validated finite point. |
| `[k]P` | `ProjectivePoint::multiply` | 256 fixed additions, doublings, and masked selections. |

### P-256 coverage

| Location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| SP 800-186 §3.2.1.3 | `p`, `b`, `n`, `G`, `h = 1`. | `field::MODULUS`, `field::CURVE_B`, `scalar::ORDER`, `point::GENERATOR_X/Y`; hexadecimal-form tests, generator-on-curve test, `[n]G = O` test. | Implemented and tested. |
| SP 800-186 §3.2.1.3 (`a = -3`) | Curve equation `y^2 = x^3 - 3x + b`. | `AffinePoint::satisfies_curve_equation`; generator accept, one-bit change reject, CAVP PKV cases. | Implemented and tested. |
| SEC 1 §2.3.5–§2.3.6 | Field elements encode as 32 big-endian bytes; decoding rejects `>= p`. | `FieldElement::from_canonical_bytes`/`to_bytes`; `p - 1` accept, `p` and all-ones reject. | Implemented and tested. |
| SEC 1 §2.3.3–§2.3.4 | Uncompressed `04 || x || y`; reject other prefixes and off-curve points. | `AffinePoint::from_bytes`/`to_bytes`; prefix, range, and curve tests, public PKV fixtures. | Implemented and tested; compressed forms deliberately unsupported. |
| Renes–Costello–Batina Algorithm 4 | Complete projective addition for `a = -3` in printed order. | `ProjectivePoint::add` with the paper's `t0..t4` names and 43 numbered steps; identity, doubling-equals-multiplication, negation, `[n]G = O`, and all downstream vectors. | Implemented and tested. |
| Fixed-structure scalar multiplication | Same operation sequence for every scalar; no secret-indexed memory. | `ProjectivePoint::multiply` uses 256 unconditional additions, doublings, and masked selects. | Implemented as a source policy; compiler/platform timing not certified. |

## ECDH P-256 source baseline

- **Publication:** NIST SP 800-56A Rev. 3, *Recommendation for Pair-Wise Key-Establishment
  Schemes Using Discrete Logarithm Cryptography*, April 2018.
  [Publication record](https://csrc.nist.gov/pubs/sp/800/56/a/r3/final);
  [doi:10.6028/NIST.SP.800-56Ar3](https://doi.org/10.6028/NIST.SP.800-56Ar3).
- **Sections owned:** §5.6.1.2.2 (key generation by testing candidates), §5.6.2.3.3 (full
  public-key validation), §5.7.1.2 (ECC CDH primitive).
- **Baseline last checked:** 2026-08-28; final, not withdrawn.
- **Published validation material:** RFC 5903 §8.1 (group 19 exchange); NIST CAVP
  `KAS_ECC_CDH_PrimitiveTest.txt` `[P-256]` (25 cases); NIST CAVP `PKV.rsp` `[P-256]`
  (12 cases). Archive checksums and conversion policy are in `tests/vectors/ecdh-p256/README.md`.

### ECDH P-256 coverage

| SP 800-56A Rev. 3 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §5.6.1.2.2 | Draw a 256-bit candidate `c`; retry while `c > n - 2`; set `d = c + 1`. | `EcdhP256PrivateKey::generate`; deterministic-source tests for `c = 0 -> d = 1`, retry after an all-ones candidate, and bounded retries. | Implemented and tested; bounded to 64 candidates. |
| §5.6.1.2 | Private key `d` in `[1, n-1]`. | `EcdhP256PrivateKey::from_bytes` via `Scalar::from_nonzero_canonical_bytes`; zero, `n`, `n - 1`, all-ones tests. | Implemented and tested. |
| §5.6.2.3.3 | Full public-key validation: not `O`, coordinates in range, curve equation, order `n`. | `EcdhP256PublicKey::from_bytes`; the uncompressed form cannot encode `O`; order `n` follows from `h = 1` and prime `n` (documented, not recomputed). Public PKV fixtures and per-cause rejection tests. | Implemented and tested. |
| §5.7.1.2 | `P = [h d]Q`, error on `O`, output `Z = x_P` as a field-element octet string. | `EcdhP256::agree`; RFC 5903 exchange, 25 CAVP CDH cases, 32 differential cases. | Implemented and tested. |
| §5.8; §6 | Derive keys from `Z` with an approved KDF and bind identities and context. | `EcdhP256SharedSecret` requires explicit exposure; rustdoc assigns KDF, transcript, and framing to TLS/SSH. | Explicit protocol obligation; not implemented in this primitive. |

The `p256` crate 0.14.0 is used only in development tests as a differential oracle.

## ECDSA P-256 source baseline

- **Publication:** NIST FIPS 186-5, *Digital Signature Standard (DSS)*, February 2023.
  [Publication record](https://csrc.nist.gov/pubs/fips/186-5/final);
  [doi:10.6028/NIST.FIPS.186-5](https://doi.org/10.6028/NIST.FIPS.186-5).
- **Sections owned:** §6.4.1 (signature generation), §6.4.2 (signature verification), and
  Appendix A.2.2 (key generation by testing candidates), with SHA-256 over P-256. §6.3's
  per-message secret is supplied by the deterministic method below, which §6.3 permits.
- **Deterministic `k`:** RFC 6979, *Deterministic Usage of the Digital Signature Algorithm
  (DSA) and Elliptic Curve Digital Signature Algorithm (ECDSA)*, August 2013, Informational,
  §2.3 (conversions) and §3.2 (generation of `k`) with HMAC-SHA-256.
  [RFC Editor record](https://www.rfc-editor.org/info/rfc6979/);
  [doi:10.17487/RFC6979](https://doi.org/10.17487/RFC6979). Errata checked 2026-08-28: none
  affect §2.3 or §3.2 for `qlen = hlen = 256`.
- **Baseline last checked:** 2026-08-28; final. FIPS 186-5 supersedes FIPS 186-4 (withdrawn
  February 2024); the signing and verification steps are unchanged between the two.
- **Published validation material:** RFC 6979 A.2.5 (P-256, SHA-256, messages `sample` and
  `test`, including the intermediate `k` values); NIST CAVP `SigGen.txt` `[P-256,SHA-256]`
  (15 cases with `d`, `k`, `Qx`, `Qy`, `R`, `S`); NIST CAVP `SigVer.rsp` `[P-256,SHA-256]`
  (15 cases with printed verdicts). Archive checksum and conversion policy are in
  `tests/vectors/ecdsa-p256/README.md`.

### ECDSA P-256 coverage

| FIPS 186-5 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §6.4.2 step 1 | Reject unless `1 <= r <= n-1` and `1 <= s <= n-1`. | `verify::verify_digest` via `Scalar::from_nonzero_canonical_bytes`; zero, `n`, and all-ones tests for each of `r` and `s`. | Implemented and tested. |
| §6.4.2 step 2 | `e` = leftmost `min(N, outlen)` bits of `H(M)`; with SHA-256 the entire digest. | `Sha256::digest` then `Scalar::reduce_bytes`; `verify_sha256` and `verify_sha256_digest` paths agree. | Implemented and tested for SHA-256 only. |
| §6.4.2 step 3 | `w = s^-1 mod n`. | `Scalar::invert` by `s^(n-2)`; inversion unit test. | Implemented and tested. |
| §6.4.2 step 4 | `u1 = e w mod n`, `u2 = r w mod n`. | `Scalar::multiply`; all published signature vectors. | Implemented and tested. |
| §6.4.2 step 5 | `R = [u1]G + [u2]Q`; reject `O`. | `ProjectivePoint::multiply` and `add`; `to_affine` returns `None` for `O`. | Implemented and tested. |
| §6.4.2 step 6 | `v = x_R mod n`; accept iff `v == r`. | `Scalar::reduce_limbs` and `equals`; RFC 6979, 15 CAVP verdicts, `(r, n - s)` acceptance, tampering rejection, 32 differential cases. | Implemented and tested. |
| §6.4.2 public-key input | `Q` must be a valid point. | `EcdsaP256VerifyingKey::from_bytes` performs SEC 1 decoding with the curve-equation check. | Implemented and tested. |
| Appendix A.2.2 | Private key by testing candidates: `c` in 256 bits, retry while `c > n - 2`, `d = c + 1`. | `scalar::generate_private_bytes` shared with ECDH; `EcdsaP256SigningKey::generate`; deterministic-source tests. | Implemented and tested. |
| §6.4.1 steps 1–3 | `e` from `H(M)`; `R = [k]G`; `r = x_R mod n`; retry if `r = 0`. | `sign::sign_with_nonce`; all 15 CAVP `SigGen` `(d, k) -> (r, s)` white-box cases. | Implemented and tested. |
| §6.4.1 step 4 | `s = k^-1 (e + r d) mod n`; retry if `s = 0`. | `sign::sign_with_nonce` via `Scalar::invert`, `add`, `multiply`; CAVP `SigGen`, RFC 6979 exact signatures, 32 byte-identical differential cases. | Implemented and tested. |
| §6.3 | Per-message secret `k` in `[1, n-1]`, unique per signature. | Deterministic RFC 6979 derivation below; no random `k` path is exposed. | Implemented as the deterministic profile only. |
| RFC 6979 §2.3.2–§2.3.4 | `bits2int` keeps 256 bits; `int2octets` is 32-byte big-endian; `bits2octets(h1) = int2octets(h1 mod n)`. | `nonce::NonceGenerator::new` via `Scalar::reduce_bytes`/`to_bytes`. | Implemented and tested. |
| RFC 6979 §3.2 steps a–g | Seed `V`, `K`; two HMAC key updates with internal octets `0x00` and `0x01`. | `NonceGenerator::new` with lettered comments; A.2.5 published `k` values. | Implemented and tested. |
| RFC 6979 §3.2 step h | Generate `T`; compare (not reduce) with `n`; retry update `K = HMAC_K(V || 0x00)`, `V = HMAC_K(V)` on rejection or `r = 0`/`s = 0`. | `NonceGenerator::candidate`/`reject` and the loop in `sign::sign_digest`; retry-state test. | Implemented and tested at source level; the retry path has no published vector. |
| Encoding of `(r, s)` | Raw `r || s` (RFC 7515 style). | `EcdsaP256Signature`; DER `ECDSA-Sig-Value` parsing is assigned to certificate/protocol layers. | Implemented as a fixed-size profile. |

The `p256` crate 0.14.0 is used only in development tests. Its RFC 6979 signatures are
byte-identical to this implementation's over 32 cases, and each side accepts the other's output.

## RSA source baseline

The RSA integer primitive is implemented once in `src/rsa/` and consumed by RSASSA-PSS
verification here and by the opt-in PKCS #1 v1.5 encodings in `rsl-crypto-legacy`.

- **Publication:** RFC 8017, *PKCS #1: RSA Cryptography Specifications Version 2.2*,
  November 2016, Informational. [RFC Editor record](https://www.rfc-editor.org/info/rfc8017/);
  [doi:10.17487/RFC8017](https://doi.org/10.17487/RFC8017).
- **Sections owned by `src/rsa/`:** §3 (key representation as `(n, e)` and `(n, d)`), §4.1
  `I2OSP`, §4.2 `OS2IP`, §5.1.1 RSAEP, §5.1.2 RSADP, §5.2.1 RSASP1, §5.2.2 RSAVP1.
- **Baseline last checked:** 2026-08-28; no errata affect the sections used.

### RSA notation mapping and coverage

| RFC 8017 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §3.1–§3.2 | Public key `(n, e)`, private key `(n, d)`; structural bounds. | `rsa::RsaPublicKey`/`RsaPrivateKey::from_components` reject zero, one, even `n`, and exponents outside `1 < x < n`. | Implemented and tested; no primality or pair validation. |
| §4.1, §4.2 | Unsigned big-endian integer/octet-string conversion with leading zeros. | `integer::BigUint::from_be_bytes`, `to_be_bytes_padded`; leading-zero test. | Implemented and tested. |
| §5.1, §5.2 | `m^e mod n` and `c^d mod n` over a `k`-byte representative below `n`. | `key::apply_primitive` and `integer::modpow` (Montgomery, base `2^32`); differential test against `num-bigint-dig` across 32–512-bit limb boundaries. | Implemented and tested; variable-time. |
| §5 (security) | Private operations need constant-time, blinded arithmetic. | `RSA_PRIMITIVE_SECURITY_STATUS = EducationalOnly`; this crate exposes no private-key RSA scheme. | Documented limitation. |

## RSASSA-PSS source baseline

- **Publication:** RFC 8017 (above), §8.1.2 RSASSA-PSS-VERIFY, §9.1.2 EMSA-PSS-VERIFY,
  Appendix B.2.1 MGF1.
- **Profile:** RFC 8446 §4.2.3 `rsa_pss_rsae_sha256` / `rsa_pss_pss_sha256` (SHA-256,
  MGF1-SHA-256, salt length equal to the digest length); FIPS 186-5 §5.4 (PSS parameters) and
  §5.1 with RFC 9325 (2048-bit minimum modulus).
- **Baseline last checked:** 2026-08-28.
- **Published validation material:** NIST CAVP `SigVerPSS_186-3.rsp` `[mod = 2048]` SHA-256
  (18 cases across several moduli, printed verdicts, 32-byte salts); NIST CAVP
  `SigGenPSS_186-3.txt` `[mod = 2048]` SHA-256 (10 cases, 20-byte salts); Project Wycheproof
  `rsa_pss_2048_sha256_mgf1_32_test.json` (108 cases). Checksums and conversion policy are in
  `tests/vectors/rsa-pss/README.md`.

### RSASSA-PSS coverage

| Location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §8.1.2 step 1 | Reject unless `len(S) = k`. | `RsaPssSha256VerifyingKey::verify_sha256_digest_with_salt_len`; short-signature unit test. | Implemented and tested. |
| §8.1.2 step 2 | `s = OS2IP(S)`; `m = RSAVP1((n, e), s)`, out-of-range is invalid; `EM = I2OSP(m, emLen)`, `emLen = ceil((modBits − 1) / 8)`. | `RsaPublicKey::apply` plus the leading-zero check when `emLen < k`; all-ones representative unit test; Wycheproof and CAVP suites. | Implemented and tested. |
| §8.1.2 step 3; §9.1.2 steps 2–14 | EMSA-PSS-VERIFY with `Hash = SHA-256`, `MGF = MGF1-SHA-256`, verifier-supplied `sLen`. | `emsa::emsa_pss_verify_sha256` with numbered steps; locally encoded defect-by-defect unit tests; 18 CAVP verdicts; 10 CAVP `sLen = 20` signatures; 108 Wycheproof results including changed-salt-length, modified-padding, special-case-hash, and wrong-primitive cases. | Implemented and tested. |
| Appendix B.2.1 | `MGF1(mgfSeed, maskLen)` with a four-byte big-endian counter. | `mgf1::mgf1_sha256`; block/truncation unit test; exercised by every PSS vector. | Implemented and tested. |
| RFC 8446 §4.2.3 | Salt length equals digest length for the TLS profile. | `verify_sha256` fixes `sLen = 32`; explicit-length entry points exist for other fixed profiles. | Implemented as the default. |
| FIPS 186-5 §5.1; RFC 9325 | Refuse moduli below 2048 bits. | `MIN_MODULUS_BITS`; `from_public_key` rejection test. | Implemented as profile policy. |
| §8.1.1; §9.1.1 | RSASSA-PSS signing and EMSA-PSS-ENCODE. | Not exposed (the private primitive is educational-only). A test-local encoder exists to exercise verify checks. | Deliberately not implemented. |

Scheme-level independent evidence for RSASSA-PSS is Wycheproof's independently generated suite;
a RustCrypto `rsa` development oracle was evaluated and not adopted because its component import
is impractically slow in unoptimized test builds. The modular exponentiation retains its
`num-bigint-dig` differential test.

## ChaCha20, Poly1305, and AEAD_CHACHA20_POLY1305 source baseline

- **Publication:** RFC 8439, *ChaCha20 and Poly1305 for IETF Protocols*, June 2018,
  Informational (obsoletes RFC 7539).
  [RFC Editor record](https://www.rfc-editor.org/info/rfc8439/);
  [doi:10.17487/RFC8439](https://doi.org/10.17487/RFC8439).
- **Errata record:** [RFC 8439 errata](https://errata.rfc-editor.org/search/?rfc_number=8439).
  Checked 2026-08-28; no verified erratum changes a vector or step used here.
- **Profile references:** RFC 8446 §5.2–§5.3 (`TLS_CHACHA20_POLY1305_SHA256`, nonce from
  sequence number); the SSH `chacha20-poly1305@openssh.com` construction is different and is
  deliberately not implemented.
- **Published validation material:** every worked example in §2.1.1, §2.2.1, §2.3.2, §2.4.2,
  §2.5.2, §2.6.2, and §2.8.2; Appendix A.1 (5), A.2 (3), A.3 (11), A.4 (3), and A.5; Project
  Wycheproof `chacha20_poly1305_test.json` (325 cases). Provenance and conversion are in
  `tests/vectors/chacha20-poly1305/README.md`.

### Notation mapping

| RFC 8439 notation | Rust representation | Meaning |
| --- | --- | --- |
| 32-bit word, `+`, `^`, `<<<` | `u32`, `wrapping_add`, `^`, `rotate_left` | §2.1 quarter-round operations. |
| state words 0–15 | `[u32; 16]` | §2.3 little-endian layout: constants, key, counter, nonce. |
| `chacha20_block` serialization | `u32::to_le_bytes` | §2.3 output bytes. |
| Poly1305 `r`, `s`, `Acc` | three 44/44/42-bit `u64` limbs; `u128` `s` and products | §2.5 integers modulo `2^130 - 5`; fold via `2^130 ≡ 5`. |
| `pad16`, `num_to_8_le_bytes` | zero slice, `u64::to_le_bytes` | §2.8 MAC input layout. |

### ChaCha20 coverage

| RFC 8439 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §2.1 | `QUARTERROUND(a, b, c, d)` in printed order with rotations 16, 12, 8, 7. | `quarter_round::quarter_round`; §2.1.1 vector. | Implemented and tested. |
| §2.2 | Quarter round on state positions. | `quarter_round_on_state`; §2.2.1 diagonal-round vector with unchanged-word check. | Implemented and tested. |
| §2.3 | State layout, ten double rounds, feed-forward, little-endian serialization. | `block::State`; §2.3.2 setup, after-20-rounds, and serialized block; A.1 vectors 1–5. | Implemented and tested. |
| §2.4 | Block-by-block keystream XOR from an initial counter; partial final block. | `ChaCha20::apply_keystream`/`encrypt`, `ChaCha20Stream`; §2.4.2 block states and ciphertext; A.2 vectors 1–3; split-agreement test. | Implemented and tested. |
| §2.4; §4 | The 32-bit counter must not wrap within one nonce. | `CounterExhausted` before any transformation; boundary tests at `2^32 - 1`. | Implemented as a hard refusal. |
| §4 | (key, nonce) uniqueness. | Typed nonce; uniqueness assigned to the protocol in rustdoc. | Protocol obligation. |

### Poly1305 coverage

| RFC 8439 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §2.5 | Split the key into `r` and `s`; clamp `r`. | `key::OneTimeKey`; §2.5.2 clamped `r` and `s`. | Implemented and tested. |
| §2.5.1 | Per block: append `0x01`, add to `Acc`, multiply by `r` mod `P`; finally add `s` and take 128 bits. | `state::Accumulator::absorb`/`finalize`; §2.5.2 every intermediate `Acc` and the tag; A.3 vectors 1–11 including all reduction edge cases. | Implemented and tested. |
| §2.5 (one-time) | A key authenticates one message only. | Documented; the AEAD derives a fresh key per nonce. | Documented obligation. |
| §4 | Compare tags without early exit. | `Poly1305::verify` ORs all byte differences. | Implemented at source level. |

### AEAD_CHACHA20_POLY1305 coverage

| RFC 8439 location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| §2.6 | One-time key = first 32 bytes of block counter 0. | `construction::one_time_key`; §2.6.2 and A.4 vectors 1–3. | Implemented and tested. |
| §2.8 | Encrypt from counter 1; MAC over `AAD || pad16 || C || pad16 || len(AAD) || len(C)`. | `construction::seal`/`authenticate`; §2.8.2 ciphertext and tag; A.5; 256 valid Wycheproof cases re-sealed byte-exact. | Implemented and tested. |
| §2.8 (decryption) | Verify the tag before releasing plaintext; uniform failure. | `construction::open`; A.5 changed tag; every-byte tampering tests; 69 invalid Wycheproof cases. | Implemented and tested. |
| §2.8; counter bound | Payload at most `(2^32 - 1) · 64` bytes. | `limits::validate_input_lengths`; boundary test. | Implemented and tested. |
| §2.8 nonce | 96-bit IETF nonce as one value. | `ChaCha20Poly1305Nonce`; other sizes unrepresentable (Wycheproof's nine wrong-size groups reject). | Implemented as a fixed profile. |

The `chacha20poly1305` crate 0.11.0 is used only in development tests: 32 varied cases agree in
both directions.

## SHA-384, HMAC-SHA-384, and HKDF-SHA-384

These three profiles reuse controlling publications already recorded above (FIPS 180-4,
FIPS 198-1 / RFC 2104, RFC 5869) with the hash parameters changed; only the deltas and their
evidence are listed here. Baselines re-checked 2026-08-28.

| Location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| FIPS 180-4 §5.3.4 | SHA-384 initial hash words. | `sha384::constants::INITIAL_HASH_VALUE`; NIST SHA-384 example initial words. | Implemented and tested. |
| FIPS 180-4 §6.5 | SHA-384 = SHA-512 preprocessing and compression from the §5.3.4 words, output `H_0 ‖ … ‖ H_5`. | `sha384::state` reuses `sha512::{final_blocks, compress_block}`; NIST one- and two-block examples including the discarded `H_6`, `H_7` (white-box); CAVP `SHA384ShortMsg.rsp` lengths 0, 8, 888, 896, 1016, 1024 bits; differential `sha2::Sha384` with fragmentation. | Implemented and tested. |
| FIPS 198-1 §2.3, Table 1 (B = 128, L = 48) | HMAC over SHA-384: 128-byte `K0`, hash keys longer than 128 bytes to 48. | `hmac::sha384::{key, state}`; Table 1 step tests (long-key digest from the `sha2` oracle, labeled differential); RFC 4231 §4.2–§4.8 cases 1–7 (case 5 as a 128-bit prefix); streaming, verification, and differential `hmac` cases. | Implemented and tested. |
| RFC 5869 §2.2–§2.3 (HashLen = 48) | HKDF-Extract and HKDF-Expand with HMAC-SHA-384; `L <= 255 · 48 = 12,240`. | `hkdf::sha384::{extract, expand, derive}`; all 83 Wycheproof `hkdf_sha384` cases (80 valid, 3 over-length rejected atomically); 12,240-byte boundary; differential `hkdf` cases. RFC 5869 publishes no SHA-384 vectors. | Implemented and tested. |

## AES-256 and AES-256-GCM

These profiles reuse the FIPS 197-upd1 and SP 800-38D baselines recorded above; only the deltas
are listed. Baselines re-checked 2026-08-28.

| Location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| FIPS 197 §5.2 Algorithm 2, `Nk = 8` | Key expansion to 60 words; `SUBWORD(ROTWORD())` with `Rcon` every eight words and the AES-256-only `SUBWORD()` when `i mod 8 = 4`. | `aes256::key_schedule::KeySchedule::expand`; all 60 Appendix A.3 words white-box. | Implemented and tested. |
| FIPS 197 Table 3, Algorithms 1 and 3, `Nr = 14` | Fourteen rounds over the unchanged state, transformations, and S-box. | `aes128::{forward, inverse}` made generic over `RoundKeySource`; `Aes256` supplies `ROUND_COUNT = 14`. All four `AES_Core256.pdf` blocks both directions; differential `aes::Aes256`. | Implemented and tested. |
| SP 800-38D §5.1 | GCM over any approved 128-bit block cipher using only `CIPH_K`. | Crate-private `gcm::block_cipher::GcmBlockCipher` implemented for `Aes128` and `Aes256`; GCTR, hash-subkey, tag, seal, and open layers are generic over it. AES-128 evidence unchanged. | Implemented and tested. |
| SP 800-38D §7–§8 (256-bit key) | Same 96-bit-IV, 128-bit-tag profile with a 32-byte key. | `gcm::api256::{Aes256Gcm, Aes256GcmKey, Aes256GcmNonce, Aes256GcmTag}`; NIST GCM-AES256 Examples 1–5 (seal, open, changed tag); 105 Wycheproof 256-bit cases (39 valid re-sealed byte-exact, 27 invalid, 39 non-96-bit nonces refused); 32 differential `aes-gcm` cases. | Implemented and tested. |

## P-384, ECDH P-384, and ECDSA P-384/SHA-384

The P-256 sections above record the controlling publications (SP 800-186, SEC 1, the
Renes–Costello–Batina formula, SP 800-56A Rev. 3, FIPS 186-5, RFC 6979); P-384 is a second
parameter set for the same executable specification. Baselines re-checked 2026-08-28.

| Location | Requirement represented | Code and evidence | Status |
| --- | --- | --- | --- |
| SP 800-186 §3.2.1.4 | `p = 2^384 - 2^128 - 2^96 + 2^32 - 1`, `n`, `b`, `G`, `h = 1`. | `curve::p384::P384` implementing the `weierstrass::Curve<6>` trait; hexadecimal-form tests for `p`, `n`, `b`, `G`; generator on curve; `[n]G = O`; RFC 5903 §8.2 initiator point from its private key. | Implemented and tested. |
| Shared arithmetic | Limb arithmetic, field and scalar residues, complete addition, fixed multiplication, SEC 1 encoding, generic over the limb count. | `curve::weierstrass::{arithmetic, field, scalar, point}`; `Modulus::new` counts reduction folds from the published prime form (9 for P-256, 2 for P-384) and the unit tests check both widths. P-256 evidence is unchanged after the refactor. | Implemented and tested. |
| SP 800-56A Rev. 3 §§5.6.1.2.2, 5.6.2.3.3, 5.7.1.2 (P-384) | Candidate-testing generation, full public-key validation, ECC CDH over 48-byte scalars and 97-byte points. | `agreement::ecdh_p384`; RFC 5903 §8.2 exchange; all 25 CAVP ECC CDH P-384 cases; 12 CAVP PKV P-384 cases; boundaries; 32 differential `p384` cases. | Implemented and tested. |
| FIPS 186-5 §6.4.1–§6.4.2, A.2.2; RFC 6979 §3.2 (P-384, SHA-384) | Deterministic signing with HMAC-SHA-384 and verification with `e` = the full 384-bit digest. | `signature::ecdsa_p384`; RFC 6979 A.2.6 `k` values and exact signatures; all 15 CAVP `SigGen` `(d, k) -> (r, s)` P-384/SHA-384 cases; all 15 CAVP `SigVer` verdicts; range and tampering boundaries; byte-identical differential signatures with `p384`. | Implemented and tested. |

## Traceability requirements for later primitives

Before implementation begins, each primitive must add its authoritative document, exact revision,
stable retrieval location, and access date here. As code is added:

- every module names the standard sections it owns and the sections it deliberately does not own;
- every standards-derived function, constant, state transition, and encoding rule cites its exact
  section, equation, table, or algorithm step;
- comments explain the mapping from notation to Rust where that mapping is not self-evident;
- tests label expectations as **published**, **derived from a published rule**, **regression**, or
  **differential** evidence;
- imported test vectors record their independent provenance under `tests/vectors/`; and
- this coverage table changes in the same commit as the implementation status it describes.

For TLS, SSH, compression formats, and error-correction schemes, protocol-specific standards
belong in their owning repositories. This ledger covers only cryptographic primitives implemented
by `rsl-crypto`; protocol repositories should link back to the primitive API they consume.
