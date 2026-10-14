//! Typed DES key, block, and expanded-schedule boundary.

use core::fmt;
use zeroize::Zeroize;

use super::{round::transform, schedule::expand};
use crate::SecretBytes;
use rsl_crypto::cipher::BlockCipher;

pub(super) const BLOCK_LEN: usize = 8;

/// One encoded 64-bit DES key, including eight ignored parity positions.
///
/// Construction accepts every bit pattern, including incorrect parity and known weak keys, to
/// preserve exact historical behavior. This owner is non-`Clone`, redacted, and zeroizing.
pub struct DesKey {
    bytes: SecretBytes<BLOCK_LEN>,
}

impl DesKey {
    /// Take ownership of eight encoded key bytes without normalizing parity.
    #[must_use]
    pub fn new(bytes: [u8; BLOCK_LEN]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }

    /// Report whether every encoded byte has odd parity, as FIPS 46-3 describes.
    ///
    /// Parity is public metadata here: the method does not expose key bytes and does not affect
    /// whether [`Des::new`] accepts the key.
    #[must_use]
    pub fn has_odd_parity(&self) -> bool {
        self.bytes
            .expose_secret()
            .iter()
            .all(|byte| byte.count_ones() % 2 == 1)
    }
}

impl fmt::Debug for DesKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DesKey([REDACTED])")
    }
}

/// One owned eight-byte DES-family input or output block.
pub struct DesBlock {
    bytes: [u8; BLOCK_LEN],
}

impl DesBlock {
    /// Take ownership of exactly one DES-family block.
    #[must_use]
    pub const fn new(bytes: [u8; BLOCK_LEN]) -> Self {
        Self { bytes }
    }

    /// Borrow all current block bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; BLOCK_LEN] {
        &self.bytes
    }

    /// Consume the wrapper and transfer its bytes to the caller.
    #[must_use]
    pub fn into_bytes(mut self) -> [u8; BLOCK_LEN] {
        core::mem::take(&mut self.bytes)
    }
}

impl AsRef<[u8]> for DesBlock {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsMut<[u8]> for DesBlock {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}

impl Drop for DesBlock {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// DES with sixteen expanded 48-bit round subkeys.
pub struct Des {
    subkeys: [u64; 16],
}

impl Des {
    /// Consume one encoded key and expand its 56 effective bits.
    #[must_use]
    pub fn new(key: DesKey) -> Self {
        let DesKey { bytes } = key;
        Self::from_encoded_key(bytes.expose_secret())
    }

    // A borrow avoids manufacturing another untracked copy of secret key bytes merely to satisfy
    // an optimization-oriented lint; key setup is not a hot path in this teaching implementation.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(super) fn from_encoded_key(bytes: &[u8; BLOCK_LEN]) -> Self {
        Self {
            subkeys: expand(bytes),
        }
    }

    /// Apply the FIPS 46-3 forward permutation to one block.
    pub fn encrypt_block(&self, block: &mut DesBlock) {
        self.encrypt_bytes(&mut block.bytes);
    }

    /// Apply the inverse permutation by visiting round subkeys in reverse order.
    pub fn decrypt_block(&self, block: &mut DesBlock) {
        self.decrypt_bytes(&mut block.bytes);
    }

    pub(super) fn encrypt_bytes(&self, bytes: &mut [u8; BLOCK_LEN]) {
        transform(bytes, &self.subkeys, false);
    }

    pub(super) fn decrypt_bytes(&self, bytes: &mut [u8; BLOCK_LEN]) {
        transform(bytes, &self.subkeys, true);
    }
}

impl BlockCipher for Des {
    type Block = DesBlock;

    fn encrypt_block(&self, block: &mut Self::Block) {
        Self::encrypt_block(self, block);
    }

    fn decrypt_block(&self, block: &mut Self::Block) {
        Self::decrypt_block(self, block);
    }
}

impl fmt::Debug for Des {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Des([REDACTED KEY SCHEDULE])")
    }
}

impl Drop for Des {
    fn drop(&mut self) {
        self.subkeys.zeroize();
    }
}
