//! Two-key and three-key Triple-DES EDE compositions from SP 800-67 Rev. 2 §2.

use core::fmt;

use super::api::{BLOCK_LEN, Des, DesBlock};
use crate::SecretBytes;
use rsl_crypto::cipher::BlockCipher;

const EDE2_KEY_LEN: usize = 16;
const EDE3_KEY_LEN: usize = 24;

/// A two-key Triple-DES bundle encoded as `K1 || K2`; the third operation reuses `K1`.
pub struct TripleDesEde2Key {
    bytes: SecretBytes<EDE2_KEY_LEN>,
}

impl TripleDesEde2Key {
    /// Take ownership of a 16-byte `K1 || K2` bundle without parity normalization.
    #[must_use]
    pub fn new(bytes: [u8; EDE2_KEY_LEN]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }
}

impl fmt::Debug for TripleDesEde2Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TripleDesEde2Key([REDACTED])")
    }
}

/// A three-key Triple-DES bundle encoded as `K1 || K2 || K3`.
pub struct TripleDesEde3Key {
    bytes: SecretBytes<EDE3_KEY_LEN>,
}

impl TripleDesEde3Key {
    /// Take ownership of a 24-byte `K1 || K2 || K3` bundle without parity normalization.
    #[must_use]
    pub fn new(bytes: [u8; EDE3_KEY_LEN]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }
}

impl fmt::Debug for TripleDesEde3Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TripleDesEde3Key([REDACTED])")
    }
}

/// Two-key Triple-DES in encrypt-decrypt-encrypt order: `E(K1, D(K2, E(K1, block)))`.
pub struct TripleDesEde2 {
    first: Des,
    second: Des,
}

impl TripleDesEde2 {
    /// Consume `K1 || K2` and expand both DES schedules.
    #[must_use]
    // Each conversion selects eight bytes from a fixed-size 16-byte owner; failure is excluded by
    // the type invariant, and documenting a public panic would misrepresent the API.
    #[allow(clippy::missing_panics_doc)]
    pub fn new(key: TripleDesEde2Key) -> Self {
        let TripleDesEde2Key { bytes } = key;
        let encoded = bytes.expose_secret();
        let first = Des::from_encoded_key(encoded[..BLOCK_LEN].try_into().expect("K1 is 8 bytes"));
        let second = Des::from_encoded_key(
            encoded[BLOCK_LEN..EDE2_KEY_LEN]
                .try_into()
                .expect("K2 is 8 bytes"),
        );
        Self { first, second }
    }

    /// Apply EDE using `K1, K2, K1`.
    pub fn encrypt_block(&self, block: &mut DesBlock) {
        self.first.encrypt_block(block);
        self.second.decrypt_block(block);
        self.first.encrypt_block(block);
    }

    /// Invert EDE using `K1, K2, K1` in reverse operation order.
    pub fn decrypt_block(&self, block: &mut DesBlock) {
        self.first.decrypt_block(block);
        self.second.encrypt_block(block);
        self.first.decrypt_block(block);
    }
}

/// Three-key Triple-DES in encrypt-decrypt-encrypt order: `E(K3, D(K2, E(K1, block)))`.
pub struct TripleDesEde3 {
    first: Des,
    second: Des,
    third: Des,
}

impl TripleDesEde3 {
    /// Consume `K1 || K2 || K3` and expand all three DES schedules.
    #[must_use]
    // Each conversion selects eight bytes from a fixed-size 24-byte owner; failure is excluded by
    // the type invariant, and documenting a public panic would misrepresent the API.
    #[allow(clippy::missing_panics_doc)]
    pub fn new(key: TripleDesEde3Key) -> Self {
        let TripleDesEde3Key { bytes } = key;
        let encoded = bytes.expose_secret();
        let first = Des::from_encoded_key(encoded[..BLOCK_LEN].try_into().expect("K1 is 8 bytes"));
        let second = Des::from_encoded_key(
            encoded[BLOCK_LEN..2 * BLOCK_LEN]
                .try_into()
                .expect("K2 is 8 bytes"),
        );
        let third =
            Des::from_encoded_key(encoded[2 * BLOCK_LEN..].try_into().expect("K3 is 8 bytes"));
        Self {
            first,
            second,
            third,
        }
    }

    /// Apply EDE using `K1`, `K2`, then `K3`.
    pub fn encrypt_block(&self, block: &mut DesBlock) {
        self.first.encrypt_block(block);
        self.second.decrypt_block(block);
        self.third.encrypt_block(block);
    }

    /// Invert EDE by applying inverse operations with `K3`, `K2`, then `K1`.
    pub fn decrypt_block(&self, block: &mut DesBlock) {
        self.third.decrypt_block(block);
        self.second.encrypt_block(block);
        self.first.decrypt_block(block);
    }
}

macro_rules! implement_block_cipher {
    ($type:ty) => {
        impl BlockCipher for $type {
            type Block = DesBlock;

            fn encrypt_block(&self, block: &mut Self::Block) {
                Self::encrypt_block(self, block);
            }

            fn decrypt_block(&self, block: &mut Self::Block) {
                Self::decrypt_block(self, block);
            }
        }
    };
}

implement_block_cipher!(TripleDesEde2);
implement_block_cipher!(TripleDesEde3);

impl fmt::Debug for TripleDesEde2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TripleDesEde2([REDACTED KEY SCHEDULES])")
    }
}

impl fmt::Debug for TripleDesEde3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TripleDesEde3([REDACTED KEY SCHEDULES])")
    }
}
