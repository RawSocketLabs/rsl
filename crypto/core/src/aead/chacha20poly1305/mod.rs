//! `AEAD_CHACHA20_POLY1305`, taught from two primitives to one authenticated record.
//!
//! # What ChaCha20-Poly1305 does
//!
//! It composes the `ChaCha20` stream cipher with the Poly1305 authenticator into authenticated
//! encryption with associated data: the payload becomes ciphertext, and a 16-byte tag binds the
//! ciphertext, the visible associated data, and the nonce to the key. RFC 8439 §2.8 defines it;
//! TLS 1.3 uses it as `TLS_CHACHA20_POLY1305_SHA256`, and it is the natural choice where AES
//! hardware is absent. It is a peer of [`Aes128Gcm`](crate::aead::gcm::Aes128Gcm) behind the
//! same [`Aead`](crate::aead::Aead) contract.
//!
//! # Inputs, output, and checked behavior
//!
//! - [`ChaCha20Poly1305Key`] owns 32 secret bytes; [`ChaCha20Poly1305Nonce`] is 12 public bytes
//!   that must never repeat under one key; [`ChaCha20Poly1305Tag`] is 16 bytes.
//! - [`ChaCha20Poly1305::seal`] returns ciphertext of the plaintext's length plus a detached tag.
//! - [`ChaCha20Poly1305::open`] verifies the tag first and only then decrypts; no plaintext
//!   exists on failure, which is reported uniformly as
//!   [`CryptoError::AuthenticationFailed`](crate::CryptoError::AuthenticationFailed).
//! - Payloads above `2^38 - 64` bytes are refused before any work.
//!
//! # RFC 8439 notation in Rust
//!
//! | RFC 8439 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `poly1305_key_gen(key, nonce)` | [`construction::one_time_key`] | §2.6: first 32 bytes of `ChaCha20` block zero. |
//! | `chacha20_aead_encrypt(aad, key, iv, constant, plaintext)` | [`construction::seal`] | §2.8.1 with the 96-bit nonce already assembled. |
//! | `pad16(x)`, `num_to_8_le_bytes` | [`construction::authenticate`] | §2.8.1 MAC input layout. |
//! | one-time key, `r`, `s` | [`Poly1305Key`](crate::mac::poly1305::Poly1305Key) | Derived per nonce, used once. |
//!
//! # Algorithm walkthrough
//!
//! 1. Derive the Poly1305 key from the `ChaCha20` block at counter 0.
//! 2. Encrypt the plaintext with `ChaCha20` starting at counter 1.
//! 3. Authenticate `AAD || pad16 || ciphertext || pad16 || len(AAD) || len(ciphertext)` with the
//!    lengths as 64-bit little-endian integers.
//! 4. To open, recompute the tag over the received ciphertext, compare without early exit, and
//!    decrypt only on success.
//!
//! # Published example
//!
//! RFC 8439 §2.8.2 seals the sunscreen quotation with AAD `50515253c0c1c2c3c4c5c6c7`:
//!
//! ```
//! use rsl_crypto::aead::chacha20poly1305::{
//!     ChaCha20Poly1305, ChaCha20Poly1305Key, ChaCha20Poly1305Nonce,
//! };
//!
//! let key = ChaCha20Poly1305Key::new(core::array::from_fn(|i| 0x80 + i as u8));
//! let nonce = ChaCha20Poly1305Nonce::new([7, 0, 0, 0, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47]);
//! let aad = [0x50, 0x51, 0x52, 0x53, 0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7];
//! let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
//!
//! let sealed = ChaCha20Poly1305::new(key).seal(&nonce, &aad, plaintext)?;
//! assert_eq!(&sealed.ciphertext()[..4], &[0xd3, 0x1a, 0x8d, 0x34]);
//! assert_eq!(
//!     sealed.tag().as_bytes(),
//!     &[0x1a, 0xe1, 0x0b, 0x59, 0x4f, 0x09, 0xe2, 0x6a, 0x7e, 0x90, 0x2e, 0xcb, 0xd0, 0x60, 0x06, 0x91],
//! );
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - Never reuse a nonce under a key: it breaks confidentiality and leaks the Poly1305 key.
//! - Do not release plaintext before the tag verifies; this API cannot.
//! - The nonce here is the complete 96-bit IETF nonce. RFC 8439's 32-bit "constant" plus 64-bit
//!   IV split, TLS's sequence-number XOR, and `chacha20-poly1305@openssh.com` (a different
//!   construction with two keys and a 64-bit nonce) are protocol layers.
//! - XChaCha20-Poly1305 is not provided.
//!
//! # Readable source map
//!
//! - [`crate::cipher::chacha20`] and [`crate::mac::poly1305`] own the primitives.
//! - [`construction`] owns §2.6 key derivation and the §2.8 composition in printed order.
//! - [`limits`] owns the counter-derived payload limit.
//! - [`api`] owns typed keys, nonces, tags, and the [`Aead`](crate::aead::Aead) contract.
//!
//! # Evidence and security status
//!
//! Public tests reproduce §2.8.2's ciphertext and tag, Appendix A.4's key-generation vectors,
//! Appendix A.5's decryption, all 325 Wycheproof `chacha20_poly1305` cases (including wrong
//! nonce sizes, modified tags, and flipped bits), and 32 development-only differential cases
//! against the `chacha20poly1305` crate 0.11.0. No side-channel or audit claim is made.

#![allow(rustdoc::private_intra_doc_links)]

mod api;
mod construction;
mod limits;

pub use api::{ChaCha20Poly1305, ChaCha20Poly1305Key, ChaCha20Poly1305Nonce, ChaCha20Poly1305Tag};

/// Current project lifecycle classification for `AEAD_CHACHA20_POLY1305`.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
