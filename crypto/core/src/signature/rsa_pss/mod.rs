//! RSASSA-PSS verification with SHA-256, taught from a signature integer to a recovered salt.
//!
//! # What RSASSA-PSS is
//!
//! RSASSA-PSS is the probabilistic RSA signature scheme of RFC 8017 §8.1. A signer hashes the
//! message, appends a random salt, hashes again, masks the result with MGF1, and applies the
//! private RSA primitive. A verifier applies the public primitive, unmasks, recovers the salt,
//! and recomputes the hash. TLS 1.3 requires it for RSA certificates as `rsa_pss_rsae_sha256`
//! (RFC 8446 §4.2.3), with `MGF1-SHA-256` and a salt length equal to the hash length.
//!
//! Only verification is implemented. Signing needs the private primitive, which this crate
//! classifies as educational until it is constant-time and blinded.
//!
//! # Inputs, output, and checked behavior
//!
//! - [`RsaPssSha256VerifyingKey`] admits an [`RsaPublicKey`](crate::rsa::RsaPublicKey) whose
//!   modulus has at least [`MIN_MODULUS_BITS`] significant bits.
//! - [`RsaPssSignature`] owns the received bytes; the length is checked against `k` at
//!   verification.
//! - Verification returns [`CryptoError::InvalidSignature`](crate::CryptoError::InvalidSignature)
//!   for a wrong length, a representative not below `n`, a leading nonzero byte where `emLen <
//!   k`, and every EMSA-PSS "inconsistent" outcome.
//! - The expected salt length is a verifier input, defaulting to `hLen = 32`. A verifier never
//!   infers it from the signature.
//!
//! # RFC 8017 notation in Rust
//!
//! | RFC 8017 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `(n, e)`, `k`, `modBits` | [`RsaPssSha256VerifyingKey`] over [`RsaPublicKey`](crate::rsa::RsaPublicKey) | Public key and its sizes. |
//! | `S`, `s = OS2IP(S)` | [`RsaPssSignature`] then `RsaPublicKey::apply` | Signature bytes and integer. |
//! | `m = RSAVP1((n, e), s)`, `EM = I2OSP(m, emLen)` | `RsaPublicKey::apply` then the leading-zero check | Public primitive and encoded message. |
//! | `mHash`, `H`, `H'` | `Sha256Digest` values | Message hash and the two salted hashes. |
//! | `maskedDB`, `dbMask`, `DB`, `PS`, `salt` | locals in [`emsa::emsa_pss_verify_sha256`] | EMSA-PSS fields. |
//! | `MGF1(mgfSeed, maskLen)` | [`mgf1::mgf1_sha256`] | Appendix B.2.1 mask generation. |
//! | `sLen` | `salt_len` parameter | Expected salt length, default `hLen`. |
//!
//! # Algorithm walkthrough
//!
//! 1. Reject unless `len(S) = k`.
//! 2. Apply the public primitive to get `m` and its `k`-byte encoding; reject if `s >= n`.
//! 3. Take the last `emLen = ceil((modBits − 1) / 8)` bytes as `EM`; reject a nonzero byte
//!    before them.
//! 4. Reject unless `EM` ends in `0xbc` and its unused leading bits are zero.
//! 5. Split `EM` into `maskedDB || H || 0xbc`; compute `dbMask = MGF1(H)`; `DB = maskedDB ⊕
//!    dbMask`; clear the unused leading bits.
//! 6. Reject unless `DB = 00…00 || 01 || salt` with exactly `sLen` salt bytes.
//! 7. Accept exactly when `H = SHA-256(00 × 8 || SHA-256(M) || salt)`.
//!
//! # Published worked example
//!
//! Project Wycheproof's `rsa_pss_2048_sha256_mgf1_32_test.json` test 1 signs the empty message:
//!
//! ```
//! use rsl_crypto::signature::rsa_pss::{RsaPssSha256VerifyingKey, RsaPssSignature};
//!
//! fn decode(hex: &str) -> Vec<u8> {
//!     (0..hex.len() / 2)
//!         .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
//!         .collect()
//! }
//!
//! let modulus = decode(concat!(
//!     "00a2b451a07d0aa5f96e455671513550514a8a5b462ebef717094fa1fee82224e637f9746d3f7cafd31878d8",
//!     "0325b6ef5a1700f65903b469429e89d6eac8845097b5ab393189db92512ed8a7711a1253facd20f79c15e824",
//!     "7f3d3e42e46e48c98e254a2fe9765313a03eff8f17e1a029397a1fa26a8dce26f490ed81299615d9814c22da",
//!     "610428e09c7d9658594266f5c021d0fceca08d945a12be82de4d1ece6b4c03145b5d3495d4ed5411eb878daf",
//!     "05fd7afc3e09ada0f1126422f590975a1969816f48698bcbba1b4d9cae79d460d8f9f85e7975005d9bc22c4e",
//!     "5ac0f7c1a45d12569a62807d3b9a02e5a530e773066f453d1f5b4c2e9cf7820283f742b9d5"
//! ));
//! let signature = RsaPssSignature::from_bytes(decode(concat!(
//!     "4f01e0c12b08625ecac89a69231906edf826380f37c959a96690d046316d68ffce9d5c471694fcebfc6b4553",
//!     "4864689256e4fc81c78e583f675d0c94b449647451e81beff01a11a516d5e5ce3f1a910437cb8a3a5096b19f",
//!     "b15f4524a35b23d89cdba12cf5b71aac1047b28c562df7c5542c34ce23a182cf7e0e231934b17294799d4487",
//!     "7a1d68ef1b8f073619b7618e6b7c22db20030d98cf591ffc3d4da5f58613ecd5ecfc3b40a1d02f40891ca436",
//!     "95cd4c088b05a8054c89c595a47e274816f35384226f74459ee63e25a1bfc03c360490552ec38343f8ace502",
//!     "f065303b00bc0ec320711b211fde92e57feb9013c3609342495ec0d7cabdec21e54acc38"
//! )));
//!
//! let key = RsaPssSha256VerifyingKey::from_components(&modulus, decode("010001"))?;
//! assert_eq!(key.modulus_bits(), 2048);
//! key.verify_sha256(b"", &signature)?;
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - Do not recover `sLen` from the signature. RFC 8017 makes it a verifier parameter, and
//!   Wycheproof rejects signatures whose salt length was changed.
//! - Do not decode `EM` by scanning for `0x01`; check the exact padding length for the expected
//!   salt, as §9.1.2 step 10 requires.
//! - Do not accept a PKCS #1 v1.5 signature under a PSS key; Wycheproof's `WrongPrimitive` case
//!   covers this.
//! - Signing, PSS with other hashes or MGFs, `RSASSA-PSS-params` ASN.1, certificates, DER, and
//!   PKCS #1 v1.5 are outside this profile.
//!
//! # Readable source map
//!
//! - [`crate::rsa`] owns the components and the public primitive.
//! - [`mgf1`] owns Appendix B.2.1.
//! - [`emsa`] owns §9.1.2 with numbered steps.
//! - [`api`] owns typed keys and signatures, §8.1.2, the modulus floor, and the generic
//!   [`Verifier`](crate::signature::Verifier) contract.
//!
//! # Evidence and security status
//!
//! Public tests cover all 18 NIST CAVP `SigVer` PSS 2048/SHA-256 verdicts (three accepts and
//! five labeled failure classes), all 10 CAVP `SigGen` PSS 2048/SHA-256 signatures with their
//! 20-byte salts, and all 108 Wycheproof `rsa_pss_2048_sha256_mgf1_32` cases including modified
//! paddings, changed salt lengths, special-case hashes, and the wrong-primitive case. Passing
//! those is not an audit.

#![allow(rustdoc::private_intra_doc_links)]

mod api;
mod emsa;
mod mgf1;

pub use api::{MIN_MODULUS_BITS, RsaPssSha256VerifyingKey, RsaPssSignature};

/// Current project lifecycle classification for RSASSA-PSS SHA-256 verification.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
