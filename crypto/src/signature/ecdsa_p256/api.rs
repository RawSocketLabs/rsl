//! Typed ECDSA P-256 verifying-key and signature boundary.
//!
//! ## Standards ownership
//!
//! SEC 1 §2.3.4 and SP 800-56A Rev. 3 §5.6.2.3.3 govern public-point decoding and validation.
//! The signature carries `r || s` as two 32-byte big-endian integers (the fixed-size form used
//! by RFC 7515 JOSE and by FIPS/CAVP fixtures); DER `ECDSA-Sig-Value` framing is left to
//! certificate and protocol layers. FIPS 186-5 §6.4.2 step order lives in [`super::verify`].

use core::fmt;

use super::verify::verify_digest;
use crate::{
    CryptoError, Result,
    curve::p256::point::{AffinePoint, ENCODED_LEN},
    digest::sha2::sha256::{Sha256, Sha256Digest},
    signature::Verifier,
};

const SIGNATURE_LEN: usize = 64;

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

impl Verifier<EcdsaP256Signature> for EcdsaP256VerifyingKey {
    /// The generic contract binds this key type to the SHA-256 profile.
    fn verify(&self, message: &[u8], signature: &EcdsaP256Signature) -> Result<()> {
        self.verify_sha256(message, signature)
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::curve::p256::{arithmetic, point::ProjectivePoint, scalar::ORDER};

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
