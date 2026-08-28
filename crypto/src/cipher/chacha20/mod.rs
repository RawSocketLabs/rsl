//! `ChaCha20`, taught from the quarter round to a keystream.
//!
//! # What `ChaCha20` is
//!
//! `ChaCha20` is a stream cipher: from a 256-bit key, a 96-bit nonce, and a 32-bit block counter
//! it generates 64-byte keystream blocks, and encryption is XOR with that keystream. RFC 8439
//! defines the IETF profile used by TLS 1.3 (`TLS_CHACHA20_POLY1305_SHA256`) and by the
//! [`chacha20poly1305`](crate::aead::chacha20poly1305) AEAD, which is the API most protocols
//! should use. On its own, `ChaCha20` provides no integrity: flipping a ciphertext bit flips the
//! same plaintext bit.
//!
//! # RFC 8439 notation in Rust
//!
//! | RFC 8439 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `QUARTERROUND(a, b, c, d)` | [`quarter_round::quarter_round`] | §2.1, four add–xor–rotate steps. |
//! | state words 0–15 | [`block::State`] | §2.3 layout: constants, key, counter, nonce. |
//! | `chacha20_block(key, counter, nonce)` | [`ChaCha20::keystream_block`] | §2.3, 20 rounds plus feed-forward. |
//! | `chacha20_encrypt(key, counter, nonce, plaintext)` | [`ChaCha20::apply_keystream`] / [`ChaCha20::encrypt`] | §2.4, block-by-block XOR. |
//! | 32-bit block counter | `u32` argument | §2.4; wrapping is refused as [`CryptoError::CounterExhausted`](crate::CryptoError::CounterExhausted). |
//!
//! # Algorithm walkthrough
//!
//! 1. Load the state: `"expand 32-byte k"` as four words, eight little-endian key words, the
//!    counter, and three little-endian nonce words.
//! 2. Run ten double rounds, each a column round (`0,4,8,12` … `3,7,11,15`) followed by a
//!    diagonal round (`0,5,10,15` … `3,4,9,14`).
//! 3. Add the initial state word by word and serialize little-endian: one keystream block.
//! 4. XOR successive blocks (counter, counter + 1, …) into the message; a partial final block
//!    uses only its leading keystream bytes.
//!
//! # Published example
//!
//! RFC 8439 §2.4.2 encrypts a sunscreen quotation with key `00..1f`, nonce
//! `00:00:00:00:00:00:00:4a:00:00:00:00`, and counter 1:
//!
//! ```
//! use rsl_crypto::cipher::chacha20::{ChaCha20, ChaCha20Key, ChaCha20Nonce};
//!
//! let key = ChaCha20Key::new(core::array::from_fn(|i| i as u8));
//! let nonce = ChaCha20Nonce::new([0, 0, 0, 0, 0, 0, 0, 0x4a, 0, 0, 0, 0]);
//! let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
//! let ciphertext = ChaCha20::new(key).encrypt(1, &nonce, plaintext)?;
//! assert_eq!(&ciphertext[..8], &[0x6e, 0x2e, 0x35, 0x9a, 0x25, 0x68, 0xf9, 0x80]);
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - Never reuse a (key, nonce) pair: XOR of two ciphertexts reveals XOR of the plaintexts.
//! - Never let the counter wrap; this API refuses rather than repeating keystream.
//! - The original 64-bit-nonce `ChaCha20`, `XChaCha20`, and reduced-round variants are different
//!   constructions and are not provided.
//!
//! # Readable source map
//!
//! - [`quarter_round`] owns §2.1 and §2.2.
//! - [`block`] owns the §2.3 state and block function.
//! - [`api`] owns typed keys and nonces, §2.4 keystream application, counter policy, and the
//!   [`StreamCipher`](crate::cipher::StreamCipher) contract.
//!
//! # Evidence and security status
//!
//! Private tests reproduce every RFC 8439 body intermediate (§2.1.1, §2.2.1, §2.3.2, §2.4.2).
//! Public tests cover Appendix A.1 block functions and A.2 encryptions, counter boundaries, and
//! stateful/one-shot agreement. Development-only differential comparison happens through the
//! AEAD against the `chacha20poly1305` crate. No side-channel or audit claim is made.

#![allow(rustdoc::private_intra_doc_links)]

mod api;
mod block;
mod quarter_round;

pub use api::{ChaCha20, ChaCha20Key, ChaCha20Nonce, ChaCha20Stream};

/// Current project lifecycle classification for `ChaCha20`.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
