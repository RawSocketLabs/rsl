//! Public, strongly typed AES-128 block-cipher boundary.
//!
//! ## Scope and standards mapping
//!
//! [NIST FIPS 197 §3.1 and Table 3][fips-197] fix both an AES-128 key and every AES block at
//! sixteen bytes. [`Aes128Key`] and [`Aes128Block`] make those equal-sized but semantically
//! different inputs distinct. [`Aes128`] expands the key through §5.2 once, then delegates block
//! transformation to §5.1 `CIPHER()` and §5.3 `INVCIPHER()`.
//!
//! This is the primitive permutation only. It supplies no nonce, mode of operation, integrity,
//! padding, framing, or multi-block message API. Directly applying a block cipher does not provide
//! safe general-purpose encryption. [`Aes128Gcm`](crate::aead::gcm::Aes128Gcm) provides the
//! authenticated composition under NIST SP 800-38D, while TLS and SSH retain their protocol state
//! and framing.
//!
//! ## Secret lifetime
//!
//! The key type and expanded schedule are non-`Clone`, redact formatting, and zeroize on drop.
//! [`Aes128::new`] consumes the input-key owner after expansion. Blocks are also non-`Clone` and
//! zeroize their internal bytes on drop because they may contain sensitive plaintext. Consuming a
//! block through [`Aes128Block::into_bytes`] transfers destruction responsibility to the caller.
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

use core::fmt;
use zeroize::Zeroize;

use super::{forward, inverse, key_schedule::KeySchedule, state::BLOCK_LEN};
use crate::{SecretBytes, cipher::BlockCipher};

/// A distinct, owned 128-bit AES-128 key.
///
/// The type intentionally implements neither `Clone`, `Copy`, `AsRef`, nor `Deref`. The only
/// public operation is construction; consuming it into [`Aes128::new`] keeps key exposure inside
/// the algorithm boundary.
///
/// # Examples
///
/// ```
/// use rsl_crypto::cipher::aes::aes128::{Aes128, Aes128Key};
///
/// let key = Aes128Key::new([0x42; 16]);
/// let cipher = Aes128::new(key); // consumes and expands the key owner
/// assert_eq!(format!("{cipher:?}"), "Aes128([REDACTED KEY SCHEDULE])");
/// ```
pub struct Aes128Key {
    bytes: SecretBytes<BLOCK_LEN>,
}

impl Aes128Key {
    /// Take ownership of exactly sixteen key bytes.
    #[must_use]
    pub fn new(bytes: [u8; BLOCK_LEN]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }
}

impl fmt::Debug for Aes128Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Aes128Key([REDACTED])")
    }
}

/// One owned 128-bit AES input or output block.
///
/// A block is distinct from a key even though both contain sixteen bytes. It may hold plaintext
/// or ciphertext depending on which operation was most recently applied.
///
/// # Examples
///
/// ```
/// use rsl_crypto::cipher::aes::aes128::Aes128Block;
///
/// let bytes = *b"one-block-input!";
/// let block = Aes128Block::new(bytes);
/// assert_eq!(block.as_bytes(), &bytes);
/// assert_eq!(block.into_bytes(), bytes);
/// ```
pub struct Aes128Block {
    bytes: [u8; BLOCK_LEN],
}

impl Aes128Block {
    /// Take ownership of exactly one complete AES block.
    #[must_use]
    pub fn new(bytes: [u8; BLOCK_LEN]) -> Self {
        Self { bytes }
    }

    /// Borrow all sixteen current block bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; BLOCK_LEN] {
        &self.bytes
    }

    /// Mutable access for the sibling AES-256 implementation, which shares this block type.
    pub(in crate::cipher::aes) fn bytes_mut(&mut self) -> &mut [u8; BLOCK_LEN] {
        &mut self.bytes
    }

    /// Consume the block and return its bytes to the caller.
    ///
    /// The caller becomes responsible for the returned array's lifetime and destruction. The
    /// consumed wrapper is left containing zeroes before its destructor runs.
    #[must_use]
    pub fn into_bytes(mut self) -> [u8; BLOCK_LEN] {
        core::mem::take(&mut self.bytes)
    }
}

impl AsRef<[u8]> for Aes128Block {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsMut<[u8]> for Aes128Block {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}

impl Drop for Aes128Block {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// AES-128 with one expanded, zeroizing key schedule.
///
/// Constructing this value performs FIPS 197 `KEYEXPANSION()` once. It can then transform any
/// number of independent blocks. It is deliberately non-`Clone` because cloning would silently
/// duplicate all 176 bytes of expanded secret key material.
///
/// # Examples
///
/// ```
/// use rsl_crypto::cipher::aes::aes128::{Aes128, Aes128Block, Aes128Key};
///
/// let cipher = Aes128::new(Aes128Key::new(*b"0123456789abcdef"));
/// let plaintext = *b"one-block-input!";
/// let mut block = Aes128Block::new(plaintext);
/// cipher.encrypt_block(&mut block);
/// assert_ne!(block.as_bytes(), &plaintext);
/// cipher.decrypt_block(&mut block);
/// assert_eq!(block.into_bytes(), plaintext);
/// ```
pub struct Aes128 {
    schedule: KeySchedule,
}

impl Aes128 {
    /// Consume one AES-128 key and expand it into the eleven round keys.
    ///
    /// See [`Aes128`] for a complete block round-trip example.
    #[must_use]
    pub fn new(key: Aes128Key) -> Self {
        // Destructuring makes the ownership transfer visible to both reviewers and lints. The
        // moved `SecretBytes` remains alive through expansion, then zeroizes at this function's
        // boundary without creating a second caller-accessible key owner.
        let Aes128Key { bytes } = key;
        let schedule = KeySchedule::expand(bytes.expose_secret());

        Self { schedule }
    }

    /// Apply FIPS 197 `CIPHER()` to one complete block in place.
    ///
    /// This raw permutation does not authenticate the result and is not a message-encryption mode.
    /// See [`Aes128`] for a complete example and [`super`] for the published known answer.
    pub fn encrypt_block(&self, block: &mut Aes128Block) {
        forward::encrypt_block(&mut block.bytes, &self.schedule);
    }

    /// Apply FIPS 197 `INVCIPHER()` to one complete block in place.
    ///
    /// This operation does not authenticate ciphertext. Protocol or AEAD code must verify its
    /// required integrity protection before releasing plaintext. See [`Aes128`] for a complete
    /// round trip.
    pub fn decrypt_block(&self, block: &mut Aes128Block) {
        inverse::decrypt_block(&mut block.bytes, &self.schedule);
    }
}

impl BlockCipher for Aes128 {
    type Block = Aes128Block;

    fn encrypt_block(&self, block: &mut Self::Block) {
        Self::encrypt_block(self, block);
    }

    fn decrypt_block(&self, block: &mut Self::Block) {
        Self::decrypt_block(self, block);
    }
}

impl fmt::Debug for Aes128 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Aes128([REDACTED KEY SCHEDULE])")
    }
}

#[cfg(test)]
mod unit {
    use super::{Aes128, Aes128Block, Aes128Key};
    use alloc::format;

    /// API-regression evidence: formatting secret-owning public types reveals no key bytes.
    #[test]
    fn secret_bearing_debug_output_is_redacted() {
        let key = Aes128Key::new([0x5a; 16]);
        assert_eq!(format!("{key:?}"), "Aes128Key([REDACTED])");

        let cipher = Aes128::new(key);
        assert_eq!(format!("{cipher:?}"), "Aes128([REDACTED KEY SCHEDULE])");
    }

    /// API-regression evidence: exposing or transferring block bytes is always explicit.
    #[test]
    fn block_exposure_and_consumption_are_explicit() {
        let bytes = [0x3c; 16];
        let block = Aes128Block::new(bytes);
        assert_eq!(block.as_bytes(), &bytes);
        assert_eq!(block.into_bytes(), bytes);
    }
}
