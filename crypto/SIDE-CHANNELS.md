# rsl-crypto — side-channel review record

**Scope and limits.** This is a *source-level* review of every place the crate branches, indexes
memory, or returns early, classified by whether the controlling value is secret. It was made by
reading the code, not by measuring it. It does **not** show that the compiled machine code is
constant-time: the compiler may reintroduce branches from masks, `==` on arrays is not guaranteed
branch-free, and cache, instruction-timing, and power behaviour are unobserved. Production use
still requires the compiler-output and platform-level analysis, fuzzing beyond the smoke runs
in `fuzz/`, and the independent audit listed in the crate's security status.

Review date: 2026-08-28. Re-review any row whose module changes.

## Classification used

- **Public** — the value is public by definition (lengths, wire encodings, verification
  outcomes, public keys, nonces, standard constants). A branch on it leaks nothing secret.
- **Secret, masked** — the value is secret and the code selects with full-width masks or
  arithmetic; no branch or secret-indexed memory access appears in the source.
- **Secret, one-shot** — a branch on a secret that runs once per key at import or generation and
  reveals only whether the input satisfied a public validity rule.
- **Secret, variable-time** — a genuine secret-dependent branch or loop; the module is labelled
  and must not be used where timing is observable.

## Findings by module

| Module | Site | Class | Note |
| --- | --- | --- | --- |
| `block_buffer` | internal fragment buffering | Public | Branches and indices depend only on public input lengths; buffered bytes are redacted and zeroized. |
| `digest::sha2::*`, `digest::sha3::*` | none secret | Public | Digests process public or key-derived data with fixed round structure; block boundaries depend only on lengths. |
| `mac::hmac::*::state` | `tags_match` | Public | OR-folds every byte, including a length mismatch, before one comparison. |
| `mac::hmac::*::key` | key longer than block → hash | Public | Branches on key *length*, not content. |
| `mac::poly1305::state` | limb arithmetic | Secret, masked | Products and carries are unconditional; final reduction uses a mask. |
| `mac::poly1305::api` | `update` buffering; `verify` | Public / Public | Branches on lengths; `verify` OR-folds byte differences. |
| `cipher::aes::aes128::{substitution, field}` | S-box | Secret, masked | Substitution is *computed* by field inversion (fixed exponent chain); no secret-indexed table. |
| `cipher::aes::*::key_schedule` | `i mod Nk` branches | Public | Depend on the loop index only. |
| `cipher::chacha20` | ARX only; counter checks | Public | Counter exhaustion branches on public counts. |
| `aead::gcm` | GHASH multiplication | Secret, masked | §6.3 Algorithm 1 iterates 128 fixed steps with masked conditional XOR. |
| `aead::gcm::tag` / `aead::chacha20poly1305` | tag verification | Public | OR-fold then one comparison; plaintext is produced only after success. |
| `aead::record` | builder stages; record splitting, numbering, and opening | Public | Branches depend on configured sizes, fragment lengths, record metadata, errors, and authentication outcomes. Pending plaintext is redacted and zeroized; the underlying AEAD produces plaintext only after tag verification. |
| `agreement::x25519::*`, `agreement::x448::*` | ladder, `cswap`, inversion | Secret, masked | Fixed 255/448-iteration ladder; RFC `0 - swap` masks; inversion exponent is public. |
| `agreement::{x25519,x448}::api` | all-zero check | Public | The result is what the peer could compute; RFC 7748 §6 explicitly permits the check. |
| `signature::ed25519::{field, point}`, `signature::ed448::{field, point}` | scalar multiplication | Secret, masked | 256/456 unconditional add+double+select steps. |
| same | `decompress` sign handling | Public | Operates on encoded public points (`A`, `R`). |
| `signature::ed25519::scalar`, `signature::ed448::scalar` | reduction and multiplication | Secret, masked | Bit-serial double-and-add with masked selects; `from_canonical_bytes` branches only on public `S`. |
| `signature::{ed25519,ed448}::api` | small-order rejection, equation check | Public | Public keys and signatures only. |
| `curve::weierstrass::arithmetic` | `Modulus::{add, subtract, multiply, power}`, `select` | Secret, masked | Fold count is fixed per modulus; conditional subtraction is masked; `power` exponents are public constants. |
| `curve::weierstrass::arithmetic` | `is_less_than`, `is_zero` | Secret, one-shot | Used at key import (`from_nonzero_canonical_bytes`) and in candidate-testing generation; each leaks one bit about whether a fresh candidate was in range, which FIPS 186-5 A.2.2 itself conditions on. |
| `curve::weierstrass::point` | `multiply` | Secret, masked | Fixed `64·N` iterations; complete addition law removes exceptional-case branches. |
| `curve::weierstrass::point` | `from_bytes`, `to_affine` identity check | Public | Public points; the identity check in ECDSA verification is on a public result. |
| `signature::ecdsa_*::sign` | `r == 0` / `s == 0` retries | Secret, one-shot | Branches on outputs that are published in the signature anyway (`r`) or whose zero-ness would be visible (`s`); probability `2^-256` and `2^-384`. |
| `signature::ecdsa_*::nonce` | RFC 6979 candidate `>= n` retry | Secret, one-shot | The retry reveals only that a candidate was out of range (probability `2^-32` for P-256, far less for P-384); RFC 6979 §3.2 defines the loop. |
| `signature::ecdsa_*::verify` | all checks | Public | Verification inputs are public. |
| `rsa::integer` | `modpow`, `Montgomery::multiply` | **Secret, variable-time** | Branches on exponent bits, uses data-dependent vector lengths, no blinding. `RSA_PRIMITIVE_SECURITY_STATUS = EducationalOnly`; this crate exposes only public-key (RSASSA-PSS verify) uses. |
| `signature::rsa_pss::emsa` | EMSA-PSS checks | Public | Verification inputs; all checks are accumulated into one flag before returning. |
| `secret::Secret` | zeroize on drop | n/a | Lifetime hygiene only; copies made by the compiler, allocator, or OS are out of scope. |

## Known gaps (recorded, not fixed)

1. **`==` on byte arrays** is used for equality of *public* values (`FieldElement::equals`,
   point equality, `Scalar::equals`). These are never on secrets in this crate, but a future
   caller must not reuse them for secret comparison; use the OR-fold pattern in
   `mac::hmac::*::state::tags_match`.
2. **Key import range checks** (`Secret, one-shot` rows) are a deliberate trade: rejecting an
   out-of-range private key at construction is required by the standards and reveals only a
   condition that a correctly generated key never triggers.
3. **RSA private operations** are variable-time by design of this readable engine and are not
   exposed by `rsl-crypto` for that reason; `rsl-crypto-legacy` exposes them as educational.
4. **Nothing here is measured.** A `dudect`-style statistical harness or a compiler-output
   inspection would be the next step; both belong to the independent audit, not to this
   record.
