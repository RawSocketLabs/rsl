//! Typed AES-256 key, block, and raw block-permutation boundary.

use core::fmt;

use super::key_schedule::{KEY_LEN, KeySchedule};
use crate::{
    SecretBytes,
    cipher::{
        BlockCipher,
        aes::aes128::{Aes128Block, forward, inverse},
    },
};

/// One owned 256-bit AES key.
///
/// Non-`Clone`, redacted, and zeroized on drop; consumed by [`Aes256::new`].
pub struct Aes256Key {
    bytes: SecretBytes<KEY_LEN>,
}

impl Aes256Key {
    /// Size of an AES-256 key in bytes.
    pub const LEN: usize = KEY_LEN;

    /// Take ownership of exactly 256 key bits.
    #[must_use]
    pub fn new(bytes: [u8; KEY_LEN]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }
}

impl fmt::Debug for Aes256Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Aes256Key([REDACTED])")
    }
}

/// AES blocks are 128 bits for every key size; AES-256 reuses the AES-128 block type.
pub type Aes256Block = Aes128Block;

/// FIPS 197 AES-256: fourteen rounds over the same state, transforms, and block as AES-128.
///
/// See the [`aes256` teaching page](crate::cipher::aes::aes256) for the published example.
pub struct Aes256 {
    schedule: KeySchedule,
}

impl Aes256 {
    /// Consume one AES-256 key and expand it into the fifteen round keys.
    #[must_use]
    pub fn new(key: Aes256Key) -> Self {
        let Aes256Key { bytes } = key;
        Self {
            schedule: KeySchedule::expand(bytes.expose_secret()),
        }
    }

    /// Apply FIPS 197 `CIPHER()` with `Nr = 14` to one block in place.
    pub fn encrypt_block(&self, block: &mut Aes256Block) {
        forward::encrypt_block(block.bytes_mut(), &self.schedule);
    }

    /// Apply FIPS 197 `INVCIPHER()` with `Nr = 14` to one block in place.
    pub fn decrypt_block(&self, block: &mut Aes256Block) {
        inverse::decrypt_block(block.bytes_mut(), &self.schedule);
    }
}

impl BlockCipher for Aes256 {
    type Block = Aes256Block;

    fn encrypt_block(&self, block: &mut Self::Block) {
        Self::encrypt_block(self, block);
    }

    fn decrypt_block(&self, block: &mut Self::Block) {
        Self::decrypt_block(self, block);
    }
}

impl fmt::Debug for Aes256 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Aes256([REDACTED KEY SCHEDULE])")
    }
}
