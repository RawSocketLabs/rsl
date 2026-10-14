# Historical cryptography standards ledger

This ledger maps every obsolete algorithm step to both its original controlling publication and
the present source establishing its deprecated or broken status. A row marked **Implemented**
means the named transformation and evidence exist; it does not mean that the algorithm is safe.

| Algorithm | Original source baseline | Present security source | Implementation status |
| --- | --- | --- | --- |
| SHA-1 | NIST FIPS 180-4, §§4.1.1, 4.2.1, 5.1.1, 5.2.1, 5.3.1, and 6.1 | NIST's 2022 transition announcement and hash-function policy | **Implemented; `Broken`.** |
| MD5 | RFC 1321, §3 and Appendix A.5 | RFC 6151 and RFC 9155 | **Implemented; `Broken`.** |
| RC4 | RFC 6229 vectors and RFC 4345 §4 historical SSH profile | RFC 7465 (TLS) and RFC 8758 (SSH) | **Implemented; `Broken`.** |
| DES / Triple-DES | Withdrawn FIPS 46-3 and SP 800-67 Rev. 2 | NIST 2005 DES withdrawal and 2024 TDEA withdrawal | **DES `Broken`; EDE2/EDE3 `Legacy`; implemented.** |
| CBC primitives | NIST SP 800-38A §6.2 | NIST revision notice and RFC 9325 TLS guidance | **Implemented; `Legacy`.** |
| RSA PKCS #1 v1.5 | RFC 8017 §§4, 5, 7.2, 8.2, 9.2, and Appendix B.1 | RFC 8017 oracle warnings, RFC 9325, RFC 9155, and NIST SHA-1 transition | **RSAES/SHA-1 `Broken`; SHA-256 `Legacy`; primitive `EducationalOnly`; implemented.** |

Every implementation change must replace the relevant planned row with exact revision, stable
links, access date, notation mapping, coverage table, vector provenance, and explicit exclusions.

## SHA-1

### Controlling and lifecycle sources

- NIST FIPS PUB 180-4, *Secure Hash Standard*, August 2015 update 1:
  <https://csrc.nist.gov/pubs/fips/180-4/upd1/final>.
- NIST, *NIST Transitioning Away from SHA-1 for All Applications*, December 15, 2022:
  <https://csrc.nist.gov/News/2022/nist-transitioning-away-from-sha-1-for-all-apps>.
- NIST, *NIST Policy on Hash Functions*:
  <https://csrc.nist.gov/projects/hash-functions/nist-policy-on-hash-functions>.
- Accessed 2026-08-27.

FIPS 180-4 remains the controlling bit-level definition. NIST's transition announcement says
SHA-1 should be removed from all software by December 31, 2030 and may remain for uses such as
verifying old signatures; that historical need is the reason this implementation is isolated.
SHA-1 is classified [`Broken`](src/digest/sha1/mod.rs), not merely legacy, because practical
collision attacks defeat an intended digest property.

### Notation and implementation map

| Publication operation | Implementation |
| --- | --- |
| §5.3.1 initial `H(0)` words | `src/digest/sha1/compression.rs::INITIAL_STATE` |
| §5.1.1 padding and §5.2.1 big-endian parsing | `src/digest/sha1/state.rs::{update_bytes,finalize}` |
| §6.1.2 schedule `W_0..W_79` | `src/digest/sha1/compression.rs::schedule` |
| §§4.1.1 and 4.2.1 phase functions/constants | `src/digest/sha1/compression.rs::round_values` |
| §6.1.2 eighty steps and feed-forward | `src/digest/sha1/compression.rs::compress` |
| 160-bit big-endian result | `src/digest/sha1/state.rs::Sha1Digest` |

### Evidence and exclusions

| Evidence | Location |
| --- | --- |
| Published `abc` and two-block examples | `tests/digests.rs::sha1_fips_examples` |
| Parsed schedule and phase-boundary intermediate checks | `src/digest/sha1/compression.rs::unit` |
| Padding boundaries, fragmented input, and independent RustCrypto comparison | `tests/digests.rs::fragmented_boundaries_match_one_shot_and_independent_oracles` |
| Atomic rejection at FIPS's maximum representable length | `src/digest/sha1/state.rs::unit::length_rejection_is_atomic` |
| Exact fixture/oracle provenance | `tests/vectors/sha1/README.md` |

This slice does not implement HMAC-SHA-1, SHA-1 collision detection, TLS's historical combined
MD5/SHA-1 constructions, certificate signatures, or protocol acceptance policy.

## MD5

### Controlling and lifecycle sources

- RFC 1321, *The MD5 Message-Digest Algorithm*, April 1992:
  <https://www.rfc-editor.org/rfc/rfc1321.html>.
- RFC 6151, *Updated Security Considerations for the MD5 Message-Digest and the HMAC-MD5
  Algorithms*, March 2011: <https://www.rfc-editor.org/rfc/rfc6151.html>.
- RFC 9155, *Deprecating MD5 and SHA-1 Signature Hashes in TLS 1.2*, December 2021:
  <https://www.rfc-editor.org/rfc/rfc9155.html>.
- Accessed 2026-08-27.

RFC 1321 controls compatibility output. RFC 6151 supersedes its security discussion and says MD5
is no longer acceptable where collision resistance is required. RFC 9155 supplies a concrete
protocol consequence without placing TLS policy in this primitive crate. MD5 is therefore
classified [`Broken`](src/digest/md5/mod.rs).

### Notation and implementation map

| Publication operation | Implementation |
| --- | --- |
| RFC 1321 §3.3 initial words | `src/digest/md5/compression.rs::INITIAL_STATE` |
| §§3.1–3.2 padding and 64-bit little-endian length | `src/digest/md5/state.rs::{update_bytes,finalize}` |
| §3.4 functions `F`, `G`, `H`, and `I` | `src/digest/md5/compression.rs::round_function` |
| §3.4 message permutations, rotations, and additive constants | `src/digest/md5/compression.rs::{message_index,SHIFTS,CONSTANTS}` |
| §3.4 four rounds and feed-forward | `src/digest/md5/compression.rs::compress` |
| §3.5 128-bit little-endian result | `src/digest/md5/state.rs::Md5Digest` |

### Evidence and exclusions

| Evidence | Location |
| --- | --- |
| All seven RFC 1321 Appendix A.5 examples | `tests/digests.rs::md5_rfc_1321_suite` |
| Constant, parsing, and four-round-index intermediate checks | `src/digest/md5/compression.rs::unit` |
| Padding boundaries, fragmented input, and independent RustCrypto comparison | `tests/digests.rs::fragmented_boundaries_match_one_shot_and_independent_oracles` |
| RFC-required modulo-`2^64` length behavior | `src/digest/md5/state.rs::unit::byte_count_wraps_as_rfc_1321_requires` |
| Exact fixture/oracle provenance | `tests/vectors/md5/README.md` |

This slice does not implement HMAC-MD5, collision detection, TLS PRFs or signatures, password
storage, challenge-response profiles, or protocol acceptance policy.

## RC4

### Controlling and lifecycle sources

- RFC 6229, *Test Vectors for the Stream Cipher RC4*, May 2011:
  <https://www.rfc-editor.org/rfc/rfc6229.html>.
- RFC 4345, *Improved Arcfour Modes for the Secure Shell (SSH) Transport Layer Protocol*, §4,
  January 2006: <https://www.rfc-editor.org/rfc/rfc4345.html#section-4>.
- RFC 7465, *Prohibiting RC4 Cipher Suites*, February 2015:
  <https://www.rfc-editor.org/rfc/rfc7465.html>.
- RFC 8758, *Deprecating RC4 in Secure Shell (SSH)*, April 2020:
  <https://www.rfc-editor.org/rfc/rfc8758.html>.
- Accessed 2026-08-27.

RFC 6229 is the controlling byte-output baseline and includes vectors produced by three
independent implementations. RFC 4345 fixes the historical SSH improved-Arcfour key sizes and
1,536-byte discard rule, but that profile remains protocol-owned. RFC 7465 requires TLS peers to
never negotiate RC4, and RFC 8758 changes all SSH RC4 algorithms to `MUST NOT`. RC4 is classified
[`Broken`](src/cipher/rc4/mod.rs) because its non-random biases have enabled practical protocol
attacks.

### Notation and implementation map

| Algorithm operation | Implementation |
| --- | --- |
| Identity permutation `S[0..255]` | `src/cipher/rc4/key_schedule.rs::schedule` |
| KSA update `j = j + S[i] + key[i mod key_len]` and swap | `src/cipher/rc4/key_schedule.rs::mix_step` |
| PRGA updates of `i`, `j`, swap, and output selection | `src/cipher/rc4/state.rs::Rc4::next_keystream_byte` |
| XOR with input | `src/cipher/rc4/state.rs::Rc4::apply_keystream` |
| Explicit historical stream drop | `src/cipher/rc4/state.rs::Rc4::discard` |
| 1–256-byte validated, zeroizing key owner | `src/cipher/rc4/state.rs::Rc4Key` |

### Evidence and exclusions

| Evidence | Location |
| --- | --- |
| RFC 6229 Key 1 output at offsets 0 through 4,111 | `tests/rc4.rs::rfc_6229_40_bit_key_covers_distant_stream_offsets` |
| RFC 4345-relevant 1,536-byte discard position | `tests/rc4.rs::explicit_ssh_era_discard_reaches_rfc_6229_offset_1536` |
| First three named KSA steps and final permutation invariant | `src/cipher/rc4/key_schedule.rs::unit` |
| Invalid key boundaries, redaction, and atomic position exhaustion | `src/cipher/rc4/state.rs::unit` |
| Fragmented input and RustCrypto differential evidence over key/input boundaries | `tests/rc4.rs::fragmented_contract_calls_match_one_shot_and_rustcrypto` |
| Bidirectional fresh-state example | `tests/rc4.rs::fresh_identical_states_reverse_the_same_bytes` |
| Exact fixture/oracle provenance | `tests/vectors/rc4/README.md` |

This slice does not choose RC4 for any protocol, derive session keys, automatically discard early
output, provide nonces or authentication, resynchronize streams, implement TLS cipher suites, or
implement SSH packet encryption. Its secret-indexed table operations are not constant-time.

## DES and Triple-DES

### Controlling and lifecycle sources

- Withdrawn FIPS 46-3, *Data Encryption Standard*, October 1999:
  <https://csrc.nist.gov/pubs/fips/46-3/final>.
- Withdrawn NIST SP 800-67 Rev. 2, *Recommendation for the Triple Data Encryption Algorithm
  (TDEA) Block Cipher*, November 2017: <https://csrc.nist.gov/pubs/sp/800/67/r2/final>.
- NIST, *Withdrawal of FIPS 46-3, FIPS 74 and FIPS 81*, May 19, 2005:
  <https://csrc.nist.gov/news/2005/withdrawal-of-fips-46-3-fips-74-and-fips-81>.
- NIST, *NIST to Withdraw Special Publication 800-67 Revision 2*, June 29, 2023:
  <https://csrc.nist.gov/News/2023/nist-to-withdraw-sp-800-67-rev-2>.
- NIST official `TDES_Core.pdf` intermediate values:
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/TDES_Core.pdf>.
- Accessed 2026-08-27.

FIPS 46-3's algorithm specification and Appendix 1 control the DES engine; its Triple-DES section
and Appendix 2 define EDE composition and keying options. SP 800-67 Rev. 2 is the final NIST TDEA
mechanics baseline. NIST withdrew DES because its 56 effective key bits no longer supply needed
security. NIST withdrew SP 800-67 on January 1, 2024, disallowing TDEA for applying new protection
while continuing to allow processing of already-protected data. DES is therefore
[`Broken`](src/cipher/des/mod.rs); EDE2 and EDE3 are [`Legacy`](src/cipher/des/mod.rs).

### Notation and implementation map

| Publication operation | Implementation |
| --- | --- |
| Initial permutation `IP` and inverse `IP^-1` | `src/cipher/des/constants.rs` and `permutation.rs::permute` |
| Expansion `E`, permutation `P`, and selection functions `S1..S8` | `src/cipher/des/constants.rs` and `round.rs::feistel` |
| Sixteen Feistel rounds and final half swap | `src/cipher/des/round.rs::transform` |
| Parity-dropping `PC-1`, rotations, and `PC-2` | `src/cipher/des/schedule.rs::expand` |
| Typed DES key, block, and schedule | `src/cipher/des/api.rs` |
| Keying option 2 (`K1, K2, K1`) EDE | `src/cipher/des/triple.rs::TripleDesEde2` |
| Keying option 1 (`K1, K2, K3`) EDE | `src/cipher/des/triple.rs::TripleDesEde3` |

### Evidence and exclusions

| Evidence | Location |
| --- | --- |
| All four official NIST EDE2 and EDE3 blocks | `tests/des.rs::nist_tdes_core_two_key_and_three_key_blocks_round_trip` |
| NIST-published first, second, and third EDE intermediate outputs | `tests/des.rs::nist_tdes_core_first_block_exposes_all_three_ede_stages` |
| Initial permutation, first round, schedule, and parity intermediates | `src/cipher/des/{permutation,round,schedule}.rs::unit` |
| DES/EDE2/EDE3 differential evidence against RustCrypto | `tests/des.rs::all_three_variants_match_rustcrypto_over_deterministic_variation` |
| Explicit odd-parity semantics | `tests/des.rs::parity_is_visible_but_does_not_gate_historical_reproduction` |
| Exact fixture/oracle provenance | `tests/vectors/des/README.md` |

This slice accepts weak/semi-weak and wrong-parity encoded keys for exact reproduction. It does
not generate keys, normalize parity, impose protocol data limits, chain blocks, pad data,
authenticate ciphertext, or define any cipher suite. Direct permutation and S-box lookups are not
constant-time.

## Cipher Block Chaining (CBC)

### Controlling and lifecycle sources

- NIST SP 800-38A, *Recommendation for Block Cipher Modes of Operation: Methods and Techniques*,
  §6.2 and Appendix F, December 2001: <https://csrc.nist.gov/pubs/sp/800/38/a/final>.
- NIST, *Decision to Revise NIST SP 800-38A*, April 28, 2023:
  <https://csrc.nist.gov/news/2023/decision-to-revise-nist-sp-800-38a>.
- NIST official `TDES_CBC.pdf` intermediate values:
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/TDES_CBC.pdf>.
- RFC 9325 / BCP 195, §4.2, current TLS deployment guidance:
  <https://www.rfc-editor.org/rfc/rfc9325.html#section-4.2>.
- Accessed 2026-08-27.

SP 800-38A §6.2 controls only the complete-block chaining equations. NIST has decided to revise
that publication but has not replaced it. RFC 9325 documents a protocol-specific consequence: a
TLS CBC suite should not be used unless Encrypt-then-MAC was successfully negotiated. The mode is
classified [`Legacy`](src/cipher/cbc.rs) in this quarantined package; that classification is not a
claim that every abstract CBC use is cryptanalytically broken.

### Notation and implementation map

| SP 800-38A operation | Implementation |
| --- | --- |
| Initial `C0 = IV` / per-direction state | `src/cipher/cbc.rs::CbcState` |
| §6.2 encryption `Cj = CIPHER_K(Pj XOR Cj-1)` | `src/cipher/cbc.rs::encrypt_blocks` |
| §6.2 decryption `Pj = CIPHER^-1_K(Cj) XOR Cj-1` | `src/cipher/cbc.rs::decrypt_blocks` |
| XOR mapping over block bytes | `src/cipher/cbc.rs::xor_in_place` |

### Evidence and exclusions

| Evidence | Location |
| --- | --- |
| NIST's four TDES-CBC blocks and final chain state | `tests/cbc.rs::nist_tdes_cbc_four_blocks_encrypt_decrypt_and_advance_chain` |
| Fragmented calls equal one continuous chain | `tests/cbc.rs::multiple_calls_are_one_continuous_chain` |
| Differential composition against RustCrypto TDEA | `tests/cbc.rs::deterministic_cbc_sequences_match_independent_rustcrypto_des` |
| Malformed runtime block sizes reject before mutation | `tests/cbc.rs::malformed_custom_block_lengths_are_rejected_before_mutation` |
| Empty-sequence boundary and machine-readable status | `tests/cbc.rs::empty_sequences_leave_state_unchanged_and_status_is_legacy` |
| Exact fixture/oracle provenance | `tests/vectors/cbc/README.md` |

This slice accepts complete typed blocks only. It deliberately excludes padding and unpadding,
ciphertext stealing, IV generation or serialization, MAC construction/order, plaintext release
policy, record framing, alerts, cipher suites, negotiation, and all TLS/SSH version rules.

## RSA PKCS #1 v1.5

### Controlling and lifecycle sources

- RFC 8017, *PKCS #1: RSA Cryptography Specifications Version 2.2*, §§4, 5, 7.2, 8.2, 9.2,
  Appendix B.1, and the security considerations, November 2016:
  <https://www.rfc-editor.org/rfc/rfc8017.html>.
- RFC 9325 / BCP 195, *Recommendations for Secure Use of Transport Layer Security (TLS) and
  Datagram Transport Layer Security (DTLS)*, §§3.1 and 4.1, November 2022:
  <https://www.rfc-editor.org/rfc/rfc9325.html>.
- RFC 9155, *Deprecating MD5 and SHA-1 Signature Hashes in TLS 1.2*, December 2021:
  <https://www.rfc-editor.org/rfc/rfc9155.html>.
- NIST, *NIST Transitioning Away from SHA-1 for All Applications*, December 15, 2022:
  <https://csrc.nist.gov/News/2022/nist-transitioning-away-from-sha-1-for-all-apps>.
- NIST CAVP, *Digital Signatures*, RSA validation-vector index:
  <https://csrc.nist.gov/projects/cryptographic-algorithm-validation-program/digital-signatures>.
- Project Wycheproof, RSA test-vector design notes:
  <https://github.com/C2SP/wycheproof/blob/main/doc/rsa.md>.
- Accessed 2026-08-27.

RFC 8017 is the controlling byte-level and mathematical definition. Section 7.2 retains
RSAES-PKCS1-v1_5 only for compatibility, recommends RSAES-OAEP for new applications, and warns
that distinguishable decoding errors can enable chosen-ciphertext attacks. RFC 9325 supplies the
protocol consequence that TLS RSA key transport should not be used and lacks forward secrecy;
the TLS response to invalid premaster secrets remains protocol-owned. RSAES-PKCS1-v1_5 is
therefore [`Broken`](src/rsa/pkcs1v15.rs).

SHA-1's practical collision failure makes RSASSA-PKCS1-v1_5 with SHA-1 [`Broken`](src/rsa/pkcs1v15.rs).
The exact SHA-256 encoding is retained as [`Legacy`](src/rsa/pkcs1v15.rs) inside this opt-in
package rather than presented as a modern default. Independently, the shared raw RSA
primitive in `rsl-crypto` is [`EducationalOnly`](src/rsa/mod.rs): its integer operations are variable-time,
branch on private exponent bits, and perform no RSA blinding. These labels distinguish compatible
output from both algorithm policy and implementation assurance.

### Notation and implementation map

| RFC 8017 operation | Implementation |
| --- | --- |
| §4.1 `I2OSP` / §4.2 `OS2IP` unsigned big-endian conversion | `rsl-crypto` `src/rsa/integer.rs::{from_be_bytes,to_be_bytes_padded}` (shared engine) |
| §§5.1–5.2 `m^e mod n` / `c^d mod n` primitives | `rsl-crypto` `src/rsa/key.rs::apply_primitive` and `src/rsa/integer.rs::modpow`, re-exported as `rsa::{RsaPublicKey,RsaPrivateKey}` |
| Base-`2^32` multiplication and Montgomery reduction supporting the primitive | `rsl-crypto` `src/rsa/integer.rs::Montgomery` |
| §7.2.1 EME encoding `00 || 02 || PS || 00 || M` | `src/rsa/pkcs1v15.rs::encode_encryption` |
| §7.2.2 EME decoding and consolidated rejection | `src/rsa/pkcs1v15.rs::{decode_encryption,decrypt_pkcs1v15}` |
| §§8.2 and 9.2 EMSA `00 || 01 || FF... || 00 || T` | `src/rsa/pkcs1v15.rs::{encode_signature,sign_encoded,verify_encoded_signature}` |
| Appendix B.1 SHA-1 and SHA-256 `DigestInfo` DER encodings | `src/rsa/pkcs1v15.rs::{SHA1_DIGEST_INFO_PREFIX,SHA256_DIGEST_INFO_PREFIX}` |
| Explicit lifecycle classifications for the primitive and three profiles | `rsl-crypto` `src/rsa/mod.rs` (primitive, re-exported) and `src/rsa/pkcs1v15.rs` |

### Evidence and exclusions

| Evidence | Location |
| --- | --- |
| Exact NIST CAVP 2048-bit SHA-256 signature generation and verification | `tests/rsa_pkcs1v15.rs::nist_cavp_sha256_signature_is_generated_and_verified_exactly` |
| Wycheproof valid RSAES decryption and invalid seven-byte padding boundary | `tests/rsa_pkcs1v15.rs::wycheproof_accepts_valid_encoding_and_uniformly_rejects_short_padding` |
| RSA modular exponentiation differential evidence against `num-bigint-dig` over 32–512-bit limb boundaries | `rsl-crypto` `src/rsa/integer.rs::unit::montgomery_modpow_matches_independent_bigint_across_boundaries` |
| SHA-1 sign/verify profile and machine-readable broken status | `tests/rsa_pkcs1v15.rs::sha1_profile_round_trips_but_remains_machine_readably_broken` |
| Encoding intermediates for EME, EMSA, complete `DigestInfo`, and eight-byte padding minimum | `src/rsa/pkcs1v15.rs::unit` |
| Invalid component, signature length, ciphertext length, block type, separator, and bounded zero-entropy cases | `tests/rsa_pkcs1v15.rs` and `src/rsa/pkcs1v15.rs::unit` |
| Exact fixture/oracle provenance and conversion policy | `tests/vectors/rsa-pkcs1v15/README.md` |

This slice imports only `n/e` or `n/d`. It deliberately excludes RSA key generation, prime or
key-pair validation, CRT and multi-prime forms, OAEP, PSS, DER/PEM, X.509, certificates, TLS
premaster-secret checks, SSH key/signature blobs, protocol negotiation, fallback, and algorithm
acceptance policy. The uniform public decryption error does not claim constant-time execution or
complete padding-oracle resistance.
