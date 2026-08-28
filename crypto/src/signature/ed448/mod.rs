//! Ed448 signatures: `EdDSA` over edwards448 with SHAKE256.
//!
//! # What Ed448 does
//!
//! Ed448 (RFC 8032 §5.2) signs with a 57-byte private key expanded by `SHAKE256(·, 114)`,
//! producing 114-byte signatures verified with a 57-byte public key. It is Ed25519's sibling at
//! the ~224-bit level: TLS 1.3 names it `ed448`, SSH `ssh-ed448`. Unlike Ed25519, every Ed448
//! hash input carries the domain prefix `dom4(F, C)`, so a context (default empty) is part of
//! the scheme itself rather than a separate variant.
//!
//! # What differs from Ed25519
//!
//! | Quantity | Ed25519 | Ed448 |
//! | --- | --- | --- |
//! | curve | twisted Edwards, `a = -1`, `d = -121665/121666` | untwisted Edwards, `a = 1`, `d = -39081` |
//! | field | `2^255 - 19` | `2^448 - 2^224 - 1` (eight 56-bit limbs) |
//! | hash | SHA-512 | `SHAKE256(·, 114)`; Ed448ph prehash `SHAKE256(M, 64)` |
//! | domain prefix | none for pure; `dom2` for ctx/ph | `dom4(F, C)` always |
//! | encodings | 32 / 64 bytes | 57 / 114 bytes; `y` in 456 bits, sign in bit 455 |
//! | cofactor | 8 | 4 |
//! | point formulas | §5.1.4 extended coordinates | §5.2.4 projective `(X, Y, Z)` |
//!
//! # Published example
//!
//! RFC 8032 §7.4 "1 octet" signs the single byte `0x03`:
//!
//! ```
//! use rsl_crypto::signature::ed448::Ed448SigningKey;
//!
//! fn decode<const N: usize>(hex: &str) -> [u8; N] {
//!     core::array::from_fn(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
//! }
//!
//! let key = Ed448SigningKey::from_seed(decode(concat!(
//!     "c4eab05d357007c632f3dbb48489924d552b08fe0c353a0d4a1f00acda2c463a",
//!     "fbea67c5e8d2877c5e3bc397a659949ef8021e954e0a12274e",
//! )));
//! assert_eq!(&key.verifying_key().as_bytes()[..4], &[0x43, 0xba, 0x28, 0xf4]);
//! let signature = key.sign(None, [0x03])?;
//! assert_eq!(&signature.as_bytes()[..4], &[0x26, 0xb8, 0xf9, 0x17]);
//! key.verifying_key().verify(None, [0x03], &signature)?;
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - Do not omit `dom4` for "pure" Ed448; it is always present (`dom4(0, "")`).
//! - The final encoding byte carries only the sign bit; any other set bit is rejected.
//! - Do not accept `S >= L` or small-order `R`/`A`; this API rejects both (strict policy, as
//!   for Ed25519).
//!
//! # Readable source map
//!
//! - [`field`] owns `GF(2^448 - 2^224 - 1)` with the §5.2.3 root-candidate trick.
//! - [`point`] owns §5.2.2–§5.2.4 encoding, decoding, addition, doubling, and fixed-structure
//!   scalar multiplication.
//! - [`scalar`] owns residues modulo `L`, including 114-byte reduction.
//! - [`api`] owns typed keys, contexts, `dom4`, signing, strict verification, and the generic
//!   contracts.
//!
//! # Evidence and security status
//!
//! All RFC 8032 §7.4 Ed448 vectors (including contexts and the 1023-byte message) and both
//! §7.5 Ed448ph vectors are reproduced exactly and verified; all Wycheproof `ed448` cases are
//! reproduced; boundaries cover contexts, small order, non-canonical `S`, and wire lengths. No
//! differential crate is used (the `RustCrypto` Ed448 implementation is pre-release only). No
//! side-channel or audit claim is made.

#![allow(rustdoc::private_intra_doc_links)]

mod api;
mod field;
mod point;
mod scalar;

pub use api::{Ed448Context, Ed448Signature, Ed448SigningKey, Ed448VerifyingKey};

/// Current project lifecycle classification for Ed448.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
