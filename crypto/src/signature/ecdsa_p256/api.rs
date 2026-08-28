//! Typed ECDSA P-256 verifying-key and signature boundary.
//!
//! ## Standards ownership
//!
//! SEC 1 §2.3.4 and SP 800-56A Rev. 3 §5.6.2.3.3 govern public-point decoding and validation.
//! The signature carries `r || s` as two 32-byte big-endian integers (the fixed-size form used
//! by RFC 7515 JOSE and by FIPS/CAVP fixtures); DER `ECDSA-Sig-Value` framing is left to
//! certificate and protocol layers. FIPS 186-5 §6.4.2 step order lives in [`super::verify`].

use core::fmt;
use zeroize::Zeroize;

use super::{sign::sign_digest, verify::verify_digest};
use crate::{
    CryptoError, Result, SecretBytes,
    curve::p256::{
        point::{AffinePoint, ENCODED_LEN, ProjectivePoint},
        scalar::{Scalar, generate_private_bytes},
    },
    digest::sha2::sha256::{Sha256, Sha256Digest},
    random::RandomSource,
    signature::{Signer, Verifier},
};

const SCALAR_LEN: usize = 32;
const SIGNATURE_LEN: usize = 64;

/// A P-256 private signing scalar `d` in `[1, n-1]`.
///
/// The owner is non-`Clone`, redacted, and zeroized on drop. Signing is deterministic
/// (RFC 6979), so the same key and message always produce the same signature and no external
/// randomness enters a signature. See the [`ecdsa_p256` teaching
/// page](crate::signature::ecdsa_p256) for a published example.
pub struct EcdsaP256SigningKey {
    bytes: SecretBytes<SCALAR_LEN>,
}

impl EcdsaP256SigningKey {
    /// Size of the big-endian scalar encoding in bytes.
    pub const LEN: usize = SCALAR_LEN;

    /// Take ownership of a big-endian scalar, rejecting zero and values `>= n`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidKey`] when the integer is outside `[1, n-1]`. The rejected
    /// bytes are cleared before returning.
    pub fn from_bytes(mut bytes: [u8; SCALAR_LEN]) -> Result<Self> {
        if Scalar::from_nonzero_canonical_bytes(&bytes).is_none() {
            bytes.zeroize();
            return Err(CryptoError::InvalidKey);
        }
        Ok(Self {
            bytes: SecretBytes::new(bytes),
        })
    }

    /// FIPS 186-5 Appendix A.2.2 key generation by testing candidates.
    ///
    /// # Errors
    ///
    /// Returns the source's error, or [`CryptoError::EntropyUnavailable`] if every permitted
    /// candidate is out of range.
    pub fn generate<R: RandomSource>(random: &mut R) -> Result<Self> {
        Ok(Self {
            bytes: SecretBytes::new(generate_private_bytes(random)?),
        })
    }

    /// Derive the public verifying point `Q = [d]G`.
    #[must_use]
    pub fn verifying_key(&self) -> EcdsaP256VerifyingKey {
        EcdsaP256VerifyingKey {
            bytes: self.public_point().to_bytes(),
        }
    }

    /// Hash the exact message bytes with SHA-256 and sign the digest deterministically.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] if SHA-256 cannot represent the message length.
    pub fn sign_sha256(&self, message: impl AsRef<[u8]>) -> Result<EcdsaP256Signature> {
        let digest = Sha256::digest(message.as_ref())?;
        self.sign_sha256_digest(&digest)
    }

    /// Sign a SHA-256 digest the caller has already computed.
    ///
    /// # Errors
    ///
    /// Propagates internal HMAC errors only; fixed-size RFC 6979 inputs cannot trigger them.
    pub fn sign_sha256_digest(&self, digest: &Sha256Digest) -> Result<EcdsaP256Signature> {
        Ok(EcdsaP256Signature(sign_digest(
            self.bytes.expose_secret(),
            digest.as_bytes(),
        )?))
    }

    fn public_point(&self) -> AffinePoint {
        ProjectivePoint::generator()
            .multiply(self.bytes.expose_secret())
            .to_affine()
            .expect("a scalar in [1, n-1] never maps the generator to infinity")
    }
}

impl fmt::Debug for EcdsaP256SigningKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EcdsaP256SigningKey([REDACTED])")
    }
}

/// A fully validated P-256 verifying point in SEC 1 uncompressed form `04 || x || y`.
///
/// See the [`ecdsa_p256` teaching page](crate::signature::ecdsa_p256) for a published example.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct EcdsaP256VerifyingKey {
    bytes: [u8; ENCODED_LEN],
}

impl EcdsaP256VerifyingKey {
    /// Size of the uncompressed encoding in bytes.
    pub const LEN: usize = ENCODED_LEN;

    /// Decode and fully validate an uncompressed point.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] for a prefix other than `0x04`, a coordinate
    /// `>= p`, or a pair off the curve.
    pub fn from_bytes(bytes: [u8; ENCODED_LEN]) -> Result<Self> {
        AffinePoint::from_bytes(&bytes).ok_or(CryptoError::InvalidPublicKey)?;
        Ok(Self { bytes })
    }

    /// Borrow the uncompressed encoding.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; ENCODED_LEN] {
        &self.bytes
    }

    /// Consume the key into its uncompressed encoding.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; ENCODED_LEN] {
        self.bytes
    }

    /// Hash the exact message bytes with SHA-256 and verify the signature over that digest.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidSignature`] on any verification failure and
    /// [`CryptoError::MessageTooLong`] if SHA-256 cannot represent the message length.
    pub fn verify_sha256(
        &self,
        message: impl AsRef<[u8]>,
        signature: &EcdsaP256Signature,
    ) -> Result<()> {
        let digest = Sha256::digest(message.as_ref())?;
        self.verify_sha256_digest(&digest, signature)
    }

    /// Verify the signature over a SHA-256 digest the caller has already computed.
    ///
    /// Use this when a protocol hashes a transcript incrementally. The digest type guarantees
    /// the input is a genuine SHA-256 output rather than arbitrary 32 bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidSignature`] on any verification failure.
    pub fn verify_sha256_digest(
        &self,
        digest: &Sha256Digest,
        signature: &EcdsaP256Signature,
    ) -> Result<()> {
        let (r, s) = signature.parts();
        verify_digest(&self.point(), digest.as_bytes(), r, s)
    }

    fn point(&self) -> AffinePoint {
        AffinePoint::from_bytes(&self.bytes).expect("validated bytes remain a curve point")
    }
}

impl fmt::Debug for EcdsaP256VerifyingKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("EcdsaP256VerifyingKey")
            .field(&self.bytes)
            .finish()
    }
}

impl AsRef<[u8]> for EcdsaP256VerifyingKey {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl TryFrom<&[u8]> for EcdsaP256VerifyingKey {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; ENCODED_LEN]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "ECDSA P-256 public key",
                expected: ENCODED_LEN,
                actual,
            })?;
        Self::from_bytes(exact)
    }
}

/// A detached 64-byte ECDSA P-256 signature `r || s`, each a 32-byte big-endian integer.
///
/// Construction preserves received bytes; range checks occur at verification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EcdsaP256Signature([u8; SIGNATURE_LEN]);

impl EcdsaP256Signature {
    /// Encoded signature size.
    pub const LEN: usize = SIGNATURE_LEN;

    /// Take ownership of an exact-size encoded signature.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; SIGNATURE_LEN]) -> Self {
        Self(bytes)
    }

    /// Borrow the exact wire representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; SIGNATURE_LEN] {
        &self.0
    }

    /// Consume the signature into its exact wire representation.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; SIGNATURE_LEN] {
        self.0
    }

    fn parts(&self) -> (&[u8; 32], &[u8; 32]) {
        let r = self.0[..32].try_into().expect("r occupies 32 bytes");
        let s = self.0[32..].try_into().expect("s occupies 32 bytes");
        (r, s)
    }
}

impl AsRef<[u8]> for EcdsaP256Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for EcdsaP256Signature {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; SIGNATURE_LEN]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "ECDSA P-256 signature",
                expected: SIGNATURE_LEN,
                actual,
            })?;
        Ok(Self(exact))
    }
}

impl Signer for EcdsaP256SigningKey {
    type Signature = EcdsaP256Signature;

    /// Deterministic RFC 6979 signing ignores `random`.
    fn sign<R: RandomSource>(&self, _random: &mut R, message: &[u8]) -> Result<Self::Signature> {
        self.sign_sha256(message)
    }
}

impl Verifier<EcdsaP256Signature> for EcdsaP256VerifyingKey {
    /// The generic contract binds this key type to the SHA-256 profile.
    fn verify(&self, message: &[u8], signature: &EcdsaP256Signature) -> Result<()> {
        self.verify_sha256(message, signature)
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::curve::p256::{arithmetic, scalar::ORDER};
    use alloc::format;

    fn generator_key() -> EcdsaP256VerifyingKey {
        let bytes = ProjectivePoint::generator().to_affine().unwrap().to_bytes();
        EcdsaP256VerifyingKey::from_bytes(bytes).unwrap()
    }

    #[test]
    fn zero_and_order_valued_r_or_s_are_rejected_before_any_arithmetic() {
        let key = generator_key();
        let n = arithmetic::to_be_bytes(&ORDER.value);
        let mut one = [0_u8; 32];
        one[31] = 1;
        for (r, s) in [([0_u8; 32], one), (one, [0_u8; 32]), (n, one), (one, n)] {
            let mut bytes = [0_u8; 64];
            bytes[..32].copy_from_slice(&r);
            bytes[32..].copy_from_slice(&s);
            assert_eq!(
                key.verify_sha256(b"message", &EcdsaP256Signature::from_bytes(bytes)),
                Err(CryptoError::InvalidSignature)
            );
        }
    }

    #[test]
    fn signing_key_is_redacted_range_checked_and_round_trips_through_verification() {
        let key = EcdsaP256SigningKey::from_bytes([0x42; 32]).unwrap();
        assert_eq!(format!("{key:?}"), "EcdsaP256SigningKey([REDACTED])");
        assert_eq!(
            EcdsaP256SigningKey::from_bytes([0; 32]).err(),
            Some(CryptoError::InvalidKey)
        );
        let signature = key.sign_sha256(b"message").unwrap();
        key.verifying_key()
            .verify_sha256(b"message", &signature)
            .unwrap();
        assert_eq!(key.sign_sha256(b"message").unwrap(), signature);
    }

    #[test]
    fn wire_slices_require_exact_lengths() {
        assert_eq!(
            EcdsaP256Signature::try_from([0_u8; 63].as_slice()),
            Err(CryptoError::InvalidLength {
                name: "ECDSA P-256 signature",
                expected: 64,
                actual: 63,
            })
        );
        assert_eq!(
            EcdsaP256VerifyingKey::try_from([0_u8; 66].as_slice()),
            Err(CryptoError::InvalidLength {
                name: "ECDSA P-256 public key",
                expected: 65,
                actual: 66,
            })
        );
    }
}
