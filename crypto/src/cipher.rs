//! Low-level symmetric ciphers and algorithm families.
//!
//! Ciphers provide confidentiality transformations, but a raw block or stream cipher does not
//! authenticate its output. Most application and protocol code should use
//! [`crate::aead::gcm::Aes128Gcm`] rather than the low-level contracts in this module.
//!
//! [`aes::aes128`] is public so the AES permutation can be inspected, tested, and reused by
//! specified constructions. Its documentation deliberately demonstrates exactly one block and
//! explains why that is not general-purpose encryption.

pub mod aes;

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
/// should prefer an authenticated construction whenever its specification permits one. No
/// concrete stream cipher is exported yet, so this contract intentionally has no runnable
/// algorithm example; a future implementation must document nonce setup, counter exhaustion,
/// seek behavior, and authenticated composition before satisfying it.
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
