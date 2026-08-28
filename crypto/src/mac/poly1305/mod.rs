//! Poly1305, taught from key clamping to a 16-byte tag.
//!
//! # What Poly1305 is
//!
//! Poly1305 is a one-time authenticator: with a fresh 32-byte key it maps a message to a 16-byte
//! tag by evaluating a polynomial modulo `P = 2^130 - 5` at the secret point `r` and adding the
//! secret `s`. It is fast and simple, but its security depends entirely on never reusing a key.
//! RFC 8439 pairs it with `ChaCha20` in the [`chacha20poly1305`](crate::aead::chacha20poly1305)
//! AEAD, which derives one key per nonce.
//!
//! # RFC 8439 notation in Rust
//!
//! | RFC 8439 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `r`, `clamp(r)` | [`key::OneTimeKey`] limbs | §2.5 first 16 key bytes, clamped, in radix `2^44`. |
//! | `s` | `OneTimeKey::s` | §2.5 last 16 key bytes as a 128-bit integer. |
//! | `P = 2^130 - 5` | folded through `2^130 ≡ 5` in [`state::Accumulator::absorb`] | The prime modulus. |
//! | `Acc = ((Acc + Block) * r) % P` | `Accumulator::absorb` | §2.5.1 per-block step; `Block` includes the `0x01` terminator. |
//! | `Acc + s`, low 128 bits | `Accumulator::finalize` | §2.5.1 final step and serialization. |
//!
//! # Published example
//!
//! RFC 8439 §2.5.2 authenticates `"Cryptographic Forum Research Group"`:
//!
//! ```
//! use rsl_crypto::mac::poly1305::{Poly1305, Poly1305Key};
//!
//! let key = Poly1305Key::new([
//!     0x85, 0xd6, 0xbe, 0x78, 0x57, 0x55, 0x6d, 0x33, 0x7f, 0x44, 0x52, 0xfe, 0x42, 0xd5,
//!     0x06, 0xa8, 0x01, 0x03, 0x80, 0x8a, 0xfb, 0x0d, 0xb2, 0xfd, 0x4a, 0xbf, 0xf6, 0xaf,
//!     0x41, 0x49, 0xf5, 0x1b,
//! ]);
//! let tag = Poly1305::authenticate(key, b"Cryptographic Forum Research Group");
//! assert_eq!(&tag.as_bytes()[..4], &[0xa8, 0x06, 0x1d, 0xc1]);
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - A Poly1305 key is single-use. It is not an HMAC-style long-term key.
//! - Tags must be compared without early exit; use [`Poly1305::verify`].
//! - Poly1305-AES and other key-derivation pairings are not provided.
//!
//! # Readable source map
//!
//! - [`key`] owns §2.5 splitting and clamping.
//! - [`state`] owns §2.5.1 accumulation, modular reduction, and finalization.
//! - [`api`] owns typed keys and tags, buffering, uniform verification, and the
//!   [`Mac`](crate::mac::Mac) contract.
//!
//! # Evidence and security status
//!
//! Private tests reproduce §2.5.2's clamped `r`, `s`, and every intermediate accumulator value.
//! Public tests cover Appendix A.3's eleven vectors, including the reduction edge cases NIST and
//! the RFC authors flagged, plus fragmentation and verification boundaries. No side-channel or
//! audit claim is made.

#![allow(rustdoc::private_intra_doc_links)]

mod api;
mod key;
mod state;

pub use api::{Poly1305, Poly1305Key, Poly1305Tag};

/// Current project lifecycle classification for Poly1305.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
