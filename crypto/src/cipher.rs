//! Low-level symmetric ciphers and algorithm families.
//!
//! Ciphers provide confidentiality transformations, but a raw block or stream cipher does not
//! authenticate its output. Most application and protocol code should use
//! [`crate::aead::gcm::Aes128Gcm`] rather than the low-level contracts in this module.
//!
//! [`aes::aes128`] is public so the AES permutation can be inspected, tested, and reused by
//! specified constructions. Its documentation deliberately demonstrates exactly one block and
//! explains why that is not general-purpose encryption. [`chacha20`] is the RFC 8439 stream
//! cipher consumed by [`crate::aead::chacha20poly1305`].

pub mod aes;
pub mod chacha20;

use crate::Result;

/// A fixed-width reversible block cipher.
///
/// This trait models only the reversible block permutation. It supplies no padding, chaining,
/// nonce, or integrity property.
///
/// # Examples
///
/// Generic code can operate on exactly one algorithm-sized block. The round trip below teaches
/// the trait contract; it is not a safe scheme for encrypting a sequence of application blocks.
///
/// ```
/// use rsl_crypto::cipher::{BlockCipher, aes::aes128::{Aes128, Aes128Block, Aes128Key}};
///
/// fn round_trip<C: BlockCipher>(cipher: &C, block: &mut C::Block) {
///     cipher.encrypt_block(block);
///     cipher.decrypt_block(block);
/// }
///
/// let cipher = Aes128::new(Aes128Key::new([0x42; 16]));
/// let mut block = Aes128Block::new(*b"one-block-input!");
/// round_trip(&cipher, &mut block);
/// assert_eq!(block.as_bytes(), b"one-block-input!");
/// ```
pub trait BlockCipher {
    /// The algorithm's fixed-size block type.
    type Block: AsRef<[u8]> + AsMut<[u8]>;

    /// Encrypt one block in place.
    fn encrypt_block(&self, block: &mut Self::Block);

    /// Decrypt one block in place.
    fn decrypt_block(&self, block: &mut Self::Block);
}

/// A stateful cipher that XORs a generated keystream into bytes.
///
/// This is a low-level confidentiality primitive, not authenticated encryption. Protocol code
/// should prefer an authenticated construction whenever its specification permits one.
/// [`chacha20::ChaCha20Stream`] is the concrete implementation; it documents nonce setup,
/// counter exhaustion, and its authenticated composition.
///
/// # Examples
///
/// ```
/// use rsl_crypto::cipher::{StreamCipher, chacha20::{ChaCha20, ChaCha20Key, ChaCha20Nonce}};
///
/// fn mask<C: StreamCipher>(cipher: &mut C, buffer: &mut [u8]) -> rsl_crypto::Result<()> {
///     cipher.apply_keystream(buffer)
/// }
///
/// let mut stream = ChaCha20::new(ChaCha20Key::new([0x42; 32])).stream(ChaCha20Nonce::new([0; 12]), 1);
/// let mut buffer = *b"plaintext";
/// mask(&mut stream, &mut buffer)?;
/// assert_ne!(&buffer, b"plaintext");
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub trait StreamCipher {
    /// XOR the next keystream bytes into `buffer`.
    ///
    /// Returns [`crate::CryptoError::CounterExhausted`] before a keystream position repeats.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CryptoError::CounterExhausted`] before a counter or position repeats.
    fn apply_keystream(&mut self, buffer: &mut [u8]) -> Result<()>;
}
