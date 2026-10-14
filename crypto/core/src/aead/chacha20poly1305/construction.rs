//! RFC 8439 §2.6 one-time key derivation and §2.8 `AEAD_CHACHA20_POLY1305` composition.
//!
//! ## Standards ownership
//!
//! §2.6: the Poly1305 key is the first 32 bytes of the `ChaCha20` block with counter zero.
//! §2.8: the ciphertext is `ChaCha20` with counter one; the tag is Poly1305 over
//! `AAD || pad16(AAD) || ciphertext || pad16(ciphertext) || len64(AAD) || len64(ciphertext)`,
//! where `pad16` pads with zeros to a 16-byte boundary and lengths are little-endian 64-bit.
//! Decryption verifies the tag over the received ciphertext before any plaintext is produced.

use alloc::vec::Vec;
use zeroize::Zeroize;

use super::limits::validate_input_lengths;
use crate::{
    Result,
    cipher::chacha20::{ChaCha20, ChaCha20Nonce},
    mac::poly1305::{Poly1305, Poly1305Key, Poly1305Tag},
};

/// §2.6: `poly1305_key_gen(key, nonce)` from `ChaCha20` block zero.
pub(super) fn one_time_key(cipher: &ChaCha20, nonce: &ChaCha20Nonce) -> Poly1305Key {
    let mut block = cipher.keystream_block(0, nonce);
    let mut key_bytes = [0_u8; 32];
    key_bytes.copy_from_slice(&block[..32]);
    block.zeroize();
    let key = Poly1305Key::new(key_bytes);
    key_bytes.zeroize();
    key
}

/// §2.8: the Poly1305 input built from AAD and ciphertext, fed incrementally.
pub(super) fn authenticate(
    key: Poly1305Key,
    associated_data: &[u8],
    ciphertext: &[u8],
) -> Poly1305 {
    const ZERO_PAD: [u8; 16] = [0; 16];
    let mut mac = Poly1305::new(key);
    mac.update(associated_data);
    mac.update(&ZERO_PAD[..pad16_len(associated_data.len())]);
    mac.update(ciphertext);
    mac.update(&ZERO_PAD[..pad16_len(ciphertext.len())]);
    mac.update((associated_data.len() as u64).to_le_bytes());
    mac.update((ciphertext.len() as u64).to_le_bytes());
    mac
}

/// Zero bytes needed to reach the next 16-byte boundary (none when already aligned).
fn pad16_len(len: usize) -> usize {
    (16 - len % 16) % 16
}

/// §2.8 encryption: returns ciphertext and tag.
///
/// # Errors
///
/// Returns [`crate::CryptoError::MessageTooLong`] before transformation when AAD or plaintext
/// exceeds the construction's limits.
pub(super) fn seal(
    cipher: &ChaCha20,
    nonce: &ChaCha20Nonce,
    associated_data: &[u8],
    plaintext: &[u8],
) -> Result<(Vec<u8>, Poly1305Tag)> {
    validate_input_lengths(associated_data.len(), plaintext.len())?;
    let key = one_time_key(cipher, nonce);
    let ciphertext = cipher.encrypt(1, nonce, plaintext)?;
    let tag = authenticate(key, associated_data, &ciphertext).finalize();
    Ok((ciphertext, tag))
}

/// §2.8 decryption with tag verification before plaintext exists.
///
/// # Errors
///
/// Returns [`crate::CryptoError::MessageTooLong`] for unsupported lengths and
/// [`crate::CryptoError::AuthenticationFailed`] when the tag does not match; no plaintext is
/// produced on failure.
pub(super) fn open(
    cipher: &ChaCha20,
    nonce: &ChaCha20Nonce,
    associated_data: &[u8],
    ciphertext: &[u8],
    tag: &Poly1305Tag,
) -> Result<Vec<u8>> {
    validate_input_lengths(associated_data.len(), ciphertext.len())?;
    let key = one_time_key(cipher, nonce);
    authenticate(key, associated_data, ciphertext).verify(tag.as_bytes())?;
    cipher.encrypt(1, nonce, ciphertext)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn pad16_reaches_the_next_boundary_only_when_needed() {
        assert_eq!(pad16_len(0), 0);
        assert_eq!(pad16_len(12), 4);
        assert_eq!(pad16_len(16), 0);
        assert_eq!(pad16_len(114), 14);
    }
}
