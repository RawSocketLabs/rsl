//! Typed public boundary for ECDH over P-384.
//!
//! ## Standards ownership
//!
//! SP 800-56A Rev. 3 §5.6.1.2.2 generates a private key by testing candidates: draw 32 random
//! bytes `c`, retry while `c > n - 2`, and set `d = c + 1`. §5.6.2.3.3 performs full public-key
//! validation before use. §5.7.1.2 defines the ECC CDH primitive `P = [h d]Q`, errors on the
//! point at infinity, and outputs `Z = x_P` as a field-element octet string.

use core::fmt;
use zeroize::Zeroize;

use crate::{
    CryptoError, Result, SecretBytes,
    agreement::KeyAgreement,
    curve::p384::{AffinePoint, ENCODED_LEN, ProjectivePoint, Scalar, generate_private_bytes},
    random::RandomSource,
};

const SCALAR_BYTES: usize = 48;

/// One owned P-384 private scalar `d` in `[1, n-1]`.
///
/// The owner is non-`Clone`, redacted, and zeroized on drop. See the
/// [`ecdh_p384` teaching page](crate::agreement::ecdh_p384) for a published example.
pub struct EcdhP384PrivateKey {
    bytes: SecretBytes<SCALAR_BYTES>,
}

impl EcdhP384PrivateKey {
    /// Size of the big-endian scalar encoding in bytes.
    pub const LEN: usize = SCALAR_BYTES;

    /// Take ownership of a big-endian scalar, rejecting zero and values `>= n`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidKey`] when the integer is outside `[1, n-1]`. The rejected
    /// bytes are cleared before returning.
    pub fn from_bytes(mut bytes: [u8; SCALAR_BYTES]) -> Result<Self> {
        if Scalar::from_nonzero_canonical_bytes(&bytes).is_none() {
            bytes.zeroize();
            return Err(CryptoError::InvalidKey);
        }
        Ok(Self {
            bytes: SecretBytes::new(bytes),
        })
    }

    /// SP 800-56A Rev. 3 §5.6.1.2.2 key generation by testing candidates.
    ///
    /// # Errors
    ///
    /// Returns the source's error, or [`CryptoError::EntropyUnavailable`] if every permitted
    /// candidate is out of range, which indicates a non-uniform source.
    pub fn generate<R: RandomSource>(random: &mut R) -> Result<Self> {
        Ok(Self {
            bytes: SecretBytes::new(generate_private_bytes(random)?),
        })
    }

    fn scalar_bytes(&self) -> &[u8; SCALAR_BYTES] {
        self.bytes.expose_secret()
    }

    /// `[d]G`, which is finite for every `d` in `[1, n-1]` because `G` has prime order `n`.
    fn public_point(&self) -> AffinePoint {
        ProjectivePoint::generator()
            .multiply(self.scalar_bytes())
            .to_affine()
            .expect("a scalar in [1, n-1] never maps the generator to infinity")
    }

    fn public_bytes(&self) -> [u8; ENCODED_LEN] {
        let mut out = [0_u8; ENCODED_LEN];
        self.public_point().write_bytes(&mut out);
        out
    }
}

impl fmt::Debug for EcdhP384PrivateKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EcdhP384PrivateKey([REDACTED])")
    }
}

/// A fully validated P-384 public point in SEC 1 uncompressed form `04 || x || y`.
///
/// Malformed, out-of-range, and off-curve encodings cannot inhabit this type.
///
/// # Examples
///
/// ```
/// use rsl_crypto::{CryptoError, agreement::ecdh_p384::EcdhP384PublicKey};
///
/// assert_eq!(
///     EcdhP384PublicKey::try_from([0_u8; 96].as_slice()),
///     Err(CryptoError::InvalidLength { name: "ECDH P-384 public key", expected: 97, actual: 96 }),
/// );
/// assert_eq!(
///     EcdhP384PublicKey::from_bytes([0_u8; 97]),
///     Err(CryptoError::InvalidPublicKey),
/// );
/// ```
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct EcdhP384PublicKey {
    bytes: [u8; ENCODED_LEN],
}

impl EcdhP384PublicKey {
    /// Size of the uncompressed encoding in bytes.
    pub const LEN: usize = ENCODED_LEN;

    /// SP 800-56A Rev. 3 §5.6.2.3.3 full public-key validation of an uncompressed encoding.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] for a prefix other than `0x04`, a coordinate
    /// `>= p`, or a pair that does not satisfy `y^2 = x^3 - 3x + b`.
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

    fn point(&self) -> AffinePoint {
        AffinePoint::from_bytes(&self.bytes).expect("validated bytes remain a curve point")
    }
}

impl fmt::Debug for EcdhP384PublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("EcdhP384PublicKey")
            .field(&self.bytes)
            .finish()
    }
}

impl AsRef<[u8]> for EcdhP384PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl TryFrom<&[u8]> for EcdhP384PublicKey {
    type Error = CryptoError;

    /// Copy and validate an exact 97-byte wire slice.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] for any other length, then the errors of
    /// [`EcdhP384PublicKey::from_bytes`].
    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; ENCODED_LEN]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "ECDH P-384 public key",
                expected: ENCODED_LEN,
                actual,
            })?;
        Self::from_bytes(exact)
    }
}

/// The 48-byte big-endian shared secret `Z = x_P`.
///
/// This owner is non-`Clone`, redacted, and zeroized on drop. It has no `AsRef` implementation
/// so exposure is visible at the KDF boundary.
pub struct EcdhP384SharedSecret {
    bytes: SecretBytes<SCALAR_BYTES>,
}

impl EcdhP384SharedSecret {
    /// Size of the shared secret in bytes.
    pub const LEN: usize = SCALAR_BYTES;

    /// Borrow the shared secret explicitly for immediate use by a key-derivation function.
    #[must_use]
    pub fn expose_secret(&self) -> &[u8; SCALAR_BYTES] {
        self.bytes.expose_secret()
    }

    /// Transfer the bytes to the caller, who becomes responsible for clearing them.
    #[must_use]
    pub fn into_inner(self) -> [u8; SCALAR_BYTES] {
        // 48-byte arrays have no `Default`, so the value is copied out and the owner's drop
        // zeroizes the original before this function returns.
        *self.bytes.expose_secret()
    }
}

impl fmt::Debug for EcdhP384SharedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EcdhP384SharedSecret([REDACTED])")
    }
}

/// SP 800-56A Rev. 3 ECC CDH public-key derivation and agreement over P-384.
#[derive(Clone, Copy, Debug, Default)]
pub struct EcdhP384;

impl EcdhP384 {
    /// Derive `Q = [d]G`.
    #[must_use]
    pub fn public_key(private_key: &EcdhP384PrivateKey) -> EcdhP384PublicKey {
        EcdhP384PublicKey {
            bytes: private_key.public_bytes(),
        }
    }

    /// SP 800-56A Rev. 3 §5.7.1.2: compute `P = [d]Q_peer` and return `Z = x_P`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] if `P` is the point at infinity.
    pub fn agree(
        private_key: &EcdhP384PrivateKey,
        peer_public_key: &EcdhP384PublicKey,
    ) -> Result<EcdhP384SharedSecret> {
        let product = peer_public_key
            .point()
            .to_projective()
            .multiply(private_key.scalar_bytes());
        let affine = product.to_affine().ok_or(CryptoError::InvalidPublicKey)?;
        let mut shared = [0_u8; SCALAR_BYTES];
        affine.x().write_bytes(&mut shared);
        Ok(EcdhP384SharedSecret {
            bytes: SecretBytes::new(shared),
        })
    }
}

impl KeyAgreement for EcdhP384 {
    type PrivateKey = EcdhP384PrivateKey;
    type PublicKey = EcdhP384PublicKey;
    type SharedSecret = EcdhP384SharedSecret;

    fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey {
        Self::public_key(private_key)
    }

    fn agree(
        private_key: &Self::PrivateKey,
        peer_public_key: &Self::PublicKey,
    ) -> Result<Self::SharedSecret> {
        Self::agree(private_key, peer_public_key)
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::format;

    struct FixedSource(u8);

    impl RandomSource for FixedSource {
        fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
            output.fill(self.0);
            Ok(())
        }
    }

    #[test]
    fn secret_owners_are_redacted_and_sizes_are_exact() {
        let private_key = EcdhP384PrivateKey::from_bytes([0x42; 48]).unwrap();
        let public_key = EcdhP384::public_key(&private_key);
        let shared = EcdhP384::agree(&private_key, &public_key).unwrap();
        assert_eq!(format!("{private_key:?}"), "EcdhP384PrivateKey([REDACTED])");
        assert_eq!(format!("{shared:?}"), "EcdhP384SharedSecret([REDACTED])");
        assert_eq!(public_key.as_bytes().len(), 97);
        assert_eq!(shared.expose_secret().len(), 48);
    }

    #[test]
    fn candidate_testing_adds_one_and_rejects_out_of_range_sources() {
        let mut one_source = FixedSource(0);
        let key = EcdhP384PrivateKey::generate(&mut one_source).unwrap();
        let mut one = [0_u8; 48];
        one[47] = 1;
        assert_eq!(key.scalar_bytes(), &one);

        let mut saturated = FixedSource(0xff);
        assert_eq!(
            EcdhP384PrivateKey::generate(&mut saturated).err(),
            Some(CryptoError::EntropyUnavailable)
        );
    }
}
