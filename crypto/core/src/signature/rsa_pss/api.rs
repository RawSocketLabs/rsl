//! Typed RSASSA-PSS verifying-key and signature boundary for the SHA-256 profile.
//!
//! ## Standards ownership
//!
//! RFC 8017 §8.1.2 RSASSA-PSS-VERIFY: check the signature length, apply RSAVP1, convert the
//! integer to `emLen = ceil((modBits - 1) / 8)` octets, and run EMSA-PSS-VERIFY. The profile
//! here fixes `Hash = SHA-256`, `MGF = MGF1-SHA-256`, and defaults `sLen = hLen = 32`, which is
//! the `rsa_pss_rsae_sha256` / `rsa_pss_pss_sha256` requirement in RFC 8446 §4.2.3. A minimum
//! modulus of 2048 bits follows FIPS 186-5 §5.1 and RFC 9325.

use alloc::vec::Vec;
use core::fmt;

use super::emsa::{HASH_LEN, emsa_pss_verify_sha256};
use crate::{
    CryptoError, Result,
    digest::sha2::sha256::{Sha256, Sha256Digest},
    rsa::RsaPublicKey,
    signature::Verifier,
};

/// Smallest modulus accepted by this profile, in significant bits.
pub const MIN_MODULUS_BITS: usize = 2048;

/// A detached RSASSA-PSS signature of exactly `k` bytes for its key.
///
/// Construction preserves received bytes; the length is checked against the key at
/// verification because `k` depends on the modulus.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RsaPssSignature(Vec<u8>);

impl RsaPssSignature {
    /// Take ownership of encoded signature bytes.
    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Borrow the wire representation.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consume the signature into its wire representation.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl AsRef<[u8]> for RsaPssSignature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<&[u8]> for RsaPssSignature {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

/// An RSA public key admitted to the RSASSA-PSS SHA-256 profile.
///
/// See the [`rsa_pss` teaching page](crate::signature::rsa_pss) for a published example.
pub struct RsaPssSha256VerifyingKey {
    key: RsaPublicKey,
}

impl RsaPssSha256VerifyingKey {
    /// Import unsigned big-endian `n` and `e` and admit them to this profile.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] for structurally invalid components or a
    /// modulus below [`MIN_MODULUS_BITS`].
    pub fn from_components(
        modulus: impl AsRef<[u8]>,
        public_exponent: impl AsRef<[u8]>,
    ) -> Result<Self> {
        Self::from_public_key(RsaPublicKey::from_components(modulus, public_exponent)?)
    }

    /// Admit an already imported public key to this profile.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] for a modulus below [`MIN_MODULUS_BITS`].
    pub fn from_public_key(key: RsaPublicKey) -> Result<Self> {
        if key.modulus_bits() < MIN_MODULUS_BITS {
            return Err(CryptoError::InvalidPublicKey);
        }
        Ok(Self { key })
    }

    /// Significant modulus size in bits (`modBits`).
    #[must_use]
    pub fn modulus_bits(&self) -> usize {
        self.key.modulus_bits()
    }

    /// Signature length `k` in bytes.
    #[must_use]
    pub fn signature_len(&self) -> usize {
        self.key.modulus_len()
    }

    /// Hash the message with SHA-256 and verify with `sLen = hLen = 32`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidSignature`] on any verification failure and
    /// [`CryptoError::MessageTooLong`] if SHA-256 cannot represent the message length.
    pub fn verify_sha256(
        &self,
        message: impl AsRef<[u8]>,
        signature: &RsaPssSignature,
    ) -> Result<()> {
        self.verify_sha256_with_salt_len(message, signature, HASH_LEN)
    }

    /// Hash the message with SHA-256 and verify with an explicit expected salt length.
    ///
    /// Use this only when a protocol or fixture fixes a salt length other than `hLen`; a
    /// verifier must know `sLen` in advance rather than recovering it from the signature.
    ///
    /// # Errors
    ///
    /// As for [`Self::verify_sha256`].
    pub fn verify_sha256_with_salt_len(
        &self,
        message: impl AsRef<[u8]>,
        signature: &RsaPssSignature,
        salt_len: usize,
    ) -> Result<()> {
        let digest = Sha256::digest(message.as_ref())?;
        self.verify_sha256_digest_with_salt_len(&digest, signature, salt_len)
    }

    /// Verify over a caller-computed SHA-256 digest with `sLen = hLen = 32`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidSignature`] on any verification failure.
    pub fn verify_sha256_digest(
        &self,
        digest: &Sha256Digest,
        signature: &RsaPssSignature,
    ) -> Result<()> {
        self.verify_sha256_digest_with_salt_len(digest, signature, HASH_LEN)
    }

    /// Verify over a caller-computed SHA-256 digest with an explicit expected salt length.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidSignature`] on any verification failure.
    pub fn verify_sha256_digest_with_salt_len(
        &self,
        digest: &Sha256Digest,
        signature: &RsaPssSignature,
        salt_len: usize,
    ) -> Result<()> {
        // §8.1.2 step 1: the signature must be exactly k octets.
        let k = self.key.modulus_len();
        if signature.as_bytes().len() != k {
            return Err(CryptoError::InvalidSignature);
        }

        // §8.1.2 step 2: s = OS2IP(S); m = RSAVP1((n, e), s), "signature representative out of
        // range" is invalid; EM = I2OSP(m, emLen) with emLen = ceil((modBits - 1) / 8).
        let representative = self
            .key
            .apply(signature.as_bytes())
            .map_err(|_| CryptoError::InvalidSignature)?;
        let em_bits = self.key.modulus_bits() - 1;
        let em_len = em_bits.div_ceil(8);
        let (leading, encoded) = representative.split_at(k - em_len);
        if leading.iter().any(|byte| *byte != 0) {
            return Err(CryptoError::InvalidSignature);
        }

        // §8.1.2 step 3: EMSA-PSS-VERIFY(M, EM, modBits - 1).
        emsa_pss_verify_sha256(digest.as_bytes(), encoded, em_bits, salt_len)
    }
}

impl fmt::Debug for RsaPssSha256VerifyingKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RsaPssSha256VerifyingKey")
            .field("modulus_bits", &self.modulus_bits())
            .finish()
    }
}

impl Verifier<RsaPssSignature> for RsaPssSha256VerifyingKey {
    /// The generic contract binds this key type to SHA-256 with `sLen = 32`.
    fn verify(&self, message: &[u8], signature: &RsaPssSignature) -> Result<()> {
        self.verify_sha256(message, signature)
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn small_moduli_are_refused_and_signature_length_is_checked_first() {
        let mut small = alloc::vec![0_u8; 128];
        small[0] = 0x80;
        small[127] = 0x01;
        assert_eq!(
            RsaPssSha256VerifyingKey::from_components(&small, [3]).err(),
            Some(CryptoError::InvalidPublicKey)
        );

        let mut modulus = alloc::vec![0_u8; 256];
        modulus[0] = 0xc0;
        modulus[255] = 0x01;
        let key = RsaPssSha256VerifyingKey::from_components(&modulus, [3]).unwrap();
        assert_eq!(key.modulus_bits(), 2048);
        assert_eq!(key.signature_len(), 256);
        assert_eq!(
            key.verify_sha256(b"m", &RsaPssSignature::from_bytes(alloc::vec![0; 255])),
            Err(CryptoError::InvalidSignature)
        );
        // An all-0xff representative is not below n and must also be reported as invalid.
        assert_eq!(
            key.verify_sha256(b"m", &RsaPssSignature::from_bytes(alloc::vec![0xff; 256])),
            Err(CryptoError::InvalidSignature)
        );
    }
}
