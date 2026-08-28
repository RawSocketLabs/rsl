//! Typed ECDSA P-384 verifying-key and signature boundary.
//!
//! ## Standards ownership
//!
//! SEC 1 §2.3.4 and SP 800-56A Rev. 3 §5.6.2.3.3 govern public-point decoding and validation.
//! The signature carries `r || s` as two 48-byte big-endian integers (the fixed-size form used
//! by RFC 7515 JOSE and by FIPS/CAVP fixtures); DER `ECDSA-Sig-Value` framing is left to
//! certificate and protocol layers. FIPS 186-5 §6.4.2 step order lives in [`super::verify`].

use core::fmt;
use zeroize::Zeroize;

use super::{sign::sign_digest, verify::verify_digest};
use crate::{
    CryptoError, Result, SecretBytes,
    curve::p384::{AffinePoint, ENCODED_LEN, ProjectivePoint, Scalar, generate_private_bytes},
    digest::sha2::sha384::{Sha384, Sha384Digest},
    random::RandomSource,
    signature::{Signer, Verifier},
};

const SCALAR_LEN: usize = 48;
const SIGNATURE_LEN: usize = 96;

/// A P-384 private signing scalar `d` in `[1, n-1]`.
///
/// The owner is non-`Clone`, redacted, and zeroized on drop. Signing is deterministic
/// (RFC 6979), so the same key and message always produce the same signature and no external
/// randomness enters a signature. See the [`ecdsa_p384` teaching
/// page](crate::signature::ecdsa_p384) for a published example.
pub struct EcdsaP384SigningKey {
    bytes: SecretBytes<SCALAR_LEN>,
}

impl EcdsaP384SigningKey {
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
    pub fn verifying_key(&self) -> EcdsaP384VerifyingKey {
        let mut bytes = [0_u8; ENCODED_LEN];
        self.public_point().write_bytes(&mut bytes);
        EcdsaP384VerifyingKey { bytes }
    }

    /// Hash the exact message bytes with SHA-384 and sign the digest deterministically.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] if SHA-384 cannot represent the message length.
    pub fn sign_sha384(&self, message: impl AsRef<[u8]>) -> Result<EcdsaP384Signature> {
        let digest = Sha384::digest(message.as_ref())?;
        self.sign_sha384_digest(&digest)
    }

    /// Sign a SHA-384 digest the caller has already computed.
    ///
    /// # Errors
    ///
    /// Propagates internal HMAC errors only; fixed-size RFC 6979 inputs cannot trigger them.
    pub fn sign_sha384_digest(&self, digest: &Sha384Digest) -> Result<EcdsaP384Signature> {
        Ok(EcdsaP384Signature(sign_digest(
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

impl fmt::Debug for EcdsaP384SigningKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EcdsaP384SigningKey([REDACTED])")
    }
}

/// A fully validated P-384 verifying point in SEC 1 uncompressed form `04 || x || y`.
///
/// See the [`ecdsa_p384` teaching page](crate::signature::ecdsa_p384) for a published example.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct EcdsaP384VerifyingKey {
    bytes: [u8; ENCODED_LEN],
}

impl EcdsaP384VerifyingKey {
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

    /// Hash the exact message bytes with SHA-384 and verify the signature over that digest.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidSignature`] on any verification failure and
    /// [`CryptoError::MessageTooLong`] if SHA-384 cannot represent the message length.
    pub fn verify_sha384(
        &self,
        message: impl AsRef<[u8]>,
        signature: &EcdsaP384Signature,
    ) -> Result<()> {
        let digest = Sha384::digest(message.as_ref())?;
        self.verify_sha384_digest(&digest, signature)
    }

    /// Verify the signature over a SHA-384 digest the caller has already computed.
    ///
    /// Use this when a protocol hashes a transcript incrementally. The digest type guarantees
    /// the input is a genuine SHA-384 output rather than arbitrary 32 bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidSignature`] on any verification failure.
    pub fn verify_sha384_digest(
        &self,
        digest: &Sha384Digest,
        signature: &EcdsaP384Signature,
    ) -> Result<()> {
        let (r, s) = signature.parts();
        verify_digest(&self.point(), digest.as_bytes(), r, s)
    }

    fn point(&self) -> AffinePoint {
        AffinePoint::from_bytes(&self.bytes).expect("validated bytes remain a curve point")
    }
}

impl fmt::Debug for EcdsaP384VerifyingKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("EcdsaP384VerifyingKey")
            .field(&self.bytes)
            .finish()
    }
}

impl AsRef<[u8]> for EcdsaP384VerifyingKey {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl TryFrom<&[u8]> for EcdsaP384VerifyingKey {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; ENCODED_LEN]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "ECDSA P-384 public key",
                expected: ENCODED_LEN,
                actual,
            })?;
        Self::from_bytes(exact)
    }
}

/// A detached 96-byte ECDSA P-384 signature `r || s`, each a 48-byte big-endian integer.
///
/// Construction preserves received bytes; range checks occur at verification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EcdsaP384Signature([u8; SIGNATURE_LEN]);

impl EcdsaP384Signature {
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

    fn parts(&self) -> (&[u8; 48], &[u8; 48]) {
        let r = self.0[..48].try_into().expect("r occupies 32 bytes");
        let s = self.0[48..].try_into().expect("s occupies 32 bytes");
        (r, s)
    }
}

impl AsRef<[u8]> for EcdsaP384Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for EcdsaP384Signature {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; SIGNATURE_LEN]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "ECDSA P-384 signature",
                expected: SIGNATURE_LEN,
                actual,
            })?;
        Ok(Self(exact))
    }
}

impl Signer for EcdsaP384SigningKey {
    type Signature = EcdsaP384Signature;

    /// Deterministic RFC 6979 signing ignores `random`.
    fn sign<R: RandomSource>(&self, _random: &mut R, message: &[u8]) -> Result<Self::Signature> {
        self.sign_sha384(message)
    }
}

impl Verifier<EcdsaP384Signature> for EcdsaP384VerifyingKey {
    /// The generic contract binds this key type to the SHA-384 profile.
    fn verify(&self, message: &[u8], signature: &EcdsaP384Signature) -> Result<()> {
        self.verify_sha384(message, signature)
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::curve::{
        p384::P384,
        weierstrass::{Curve, arithmetic},
    };
    use alloc::format;

    fn generator_key() -> EcdsaP384VerifyingKey {
        let mut bytes = [0_u8; ENCODED_LEN];
        ProjectivePoint::generator()
            .to_affine()
            .unwrap()
            .write_bytes(&mut bytes);
        EcdsaP384VerifyingKey::from_bytes(bytes).unwrap()
    }

    #[test]
    fn zero_and_order_valued_r_or_s_are_rejected_before_any_arithmetic() {
        let key = generator_key();
        let mut n = [0_u8; 48];
        arithmetic::write_be_bytes(&P384::ORDER.value, &mut n);
        let mut one = [0_u8; 48];
        one[47] = 1;
        for (r, s) in [([0_u8; 48], one), (one, [0_u8; 48]), (n, one), (one, n)] {
            let mut bytes = [0_u8; 96];
            bytes[..48].copy_from_slice(&r);
            bytes[48..].copy_from_slice(&s);
            assert_eq!(
                key.verify_sha384(b"message", &EcdsaP384Signature::from_bytes(bytes)),
                Err(CryptoError::InvalidSignature)
            );
        }
    }

    #[test]
    fn signing_key_is_redacted_range_checked_and_round_trips_through_verification() {
        let key = EcdsaP384SigningKey::from_bytes([0x42; 48]).unwrap();
        assert_eq!(format!("{key:?}"), "EcdsaP384SigningKey([REDACTED])");
        assert_eq!(
            EcdsaP384SigningKey::from_bytes([0; 48]).err(),
            Some(CryptoError::InvalidKey)
        );
        let signature = key.sign_sha384(b"message").unwrap();
        key.verifying_key()
            .verify_sha384(b"message", &signature)
            .unwrap();
        assert_eq!(key.sign_sha384(b"message").unwrap(), signature);
    }

    #[test]
    fn wire_slices_require_exact_lengths() {
        assert_eq!(
            EcdsaP384Signature::try_from([0_u8; 95].as_slice()),
            Err(CryptoError::InvalidLength {
                name: "ECDSA P-384 signature",
                expected: 96,
                actual: 95,
            })
        );
        assert_eq!(
            EcdsaP384VerifyingKey::try_from([0_u8; 98].as_slice()),
            Err(CryptoError::InvalidLength {
                name: "ECDSA P-384 public key",
                expected: 97,
                actual: 98,
            })
        );
    }
}
