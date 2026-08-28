//! Typed public boundary for ECDH over P-256.
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
    curve::p256::{
        arithmetic,
        point::{AffinePoint, ENCODED_LEN, ProjectivePoint},
        scalar::{ORDER, Scalar},
    },
    random::RandomSource,
};

const SCALAR_BYTES: usize = 32;

/// Candidate draws permitted before generation reports the entropy source as unusable.
///
/// A candidate exceeds `n - 2` with probability about `2^-32`, so a conforming source never
/// approaches this bound.
const MAX_CANDIDATES: usize = 64;

/// One owned P-256 private scalar `d` in `[1, n-1]`.
///
/// The owner is non-`Clone`, redacted, and zeroized on drop. See the
/// [`ecdh_p256` teaching page](crate::agreement::ecdh_p256) for a published example.
pub struct EcdhP256PrivateKey {
    bytes: SecretBytes<SCALAR_BYTES>,
}

impl EcdhP256PrivateKey {
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
        let (n_minus_two, _) = arithmetic::subtract_limbs(&ORDER.value, &[2, 0, 0, 0]);
        for _ in 0..MAX_CANDIDATES {
            let mut candidate = [0_u8; SCALAR_BYTES];
            if let Err(error) = random.fill_bytes(&mut candidate) {
                candidate.zeroize();
                return Err(error);
            }
            let mut limbs = arithmetic::from_be_bytes(&candidate);
            candidate.zeroize();
            // Accept exactly the candidates `c <= n - 2`, then return `d = c + 1`.
            if arithmetic::is_less_than(&n_minus_two, &limbs) {
                limbs.zeroize();
                continue;
            }
            let (mut d, _) = arithmetic::add_limbs(&limbs, &[1, 0, 0, 0]);
            limbs.zeroize();
            let bytes = arithmetic::to_be_bytes(&d);
            d.zeroize();
            return Ok(Self {
                bytes: SecretBytes::new(bytes),
            });
        }
        Err(CryptoError::EntropyUnavailable)
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
}

impl fmt::Debug for EcdhP256PrivateKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EcdhP256PrivateKey([REDACTED])")
    }
}

/// A fully validated P-256 public point in SEC 1 uncompressed form `04 || x || y`.
///
/// Malformed, out-of-range, and off-curve encodings cannot inhabit this type.
///
/// # Examples
///
/// ```
/// use rsl_crypto::{CryptoError, agreement::ecdh_p256::EcdhP256PublicKey};
///
/// assert_eq!(
///     EcdhP256PublicKey::try_from([0_u8; 64].as_slice()),
///     Err(CryptoError::InvalidLength { name: "ECDH P-256 public key", expected: 65, actual: 64 }),
/// );
/// assert_eq!(
///     EcdhP256PublicKey::from_bytes([0_u8; 65]),
///     Err(CryptoError::InvalidPublicKey),
/// );
/// ```
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct EcdhP256PublicKey {
    bytes: [u8; ENCODED_LEN],
}

impl EcdhP256PublicKey {
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

impl fmt::Debug for EcdhP256PublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("EcdhP256PublicKey")
            .field(&self.bytes)
            .finish()
    }
}

impl AsRef<[u8]> for EcdhP256PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl TryFrom<&[u8]> for EcdhP256PublicKey {
    type Error = CryptoError;

    /// Copy and validate an exact 65-byte wire slice.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] for any other length, then the errors of
    /// [`EcdhP256PublicKey::from_bytes`].
    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; ENCODED_LEN]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "ECDH P-256 public key",
                expected: ENCODED_LEN,
                actual,
            })?;
        Self::from_bytes(exact)
    }
}

/// The 32-byte big-endian shared secret `Z = x_P`.
///
/// This owner is non-`Clone`, redacted, and zeroized on drop. It has no `AsRef` implementation
/// so exposure is visible at the KDF boundary.
pub struct EcdhP256SharedSecret {
    bytes: SecretBytes<SCALAR_BYTES>,
}

impl EcdhP256SharedSecret {
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
        self.bytes.into_inner()
    }
}

impl fmt::Debug for EcdhP256SharedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EcdhP256SharedSecret([REDACTED])")
    }
}

/// SP 800-56A Rev. 3 ECC CDH public-key derivation and agreement over P-256.
#[derive(Clone, Copy, Debug, Default)]
pub struct EcdhP256;

impl EcdhP256 {
    /// Derive `Q = [d]G`.
    #[must_use]
    pub fn public_key(private_key: &EcdhP256PrivateKey) -> EcdhP256PublicKey {
        EcdhP256PublicKey {
            bytes: private_key.public_point().to_bytes(),
        }
    }

    /// SP 800-56A Rev. 3 §5.7.1.2: compute `P = [d]Q_peer` and return `Z = x_P`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] if `P` is the point at infinity.
    pub fn agree(
        private_key: &EcdhP256PrivateKey,
        peer_public_key: &EcdhP256PublicKey,
    ) -> Result<EcdhP256SharedSecret> {
        let product = peer_public_key
            .point()
            .to_projective()
            .multiply(private_key.scalar_bytes());
        let affine = product.to_affine().ok_or(CryptoError::InvalidPublicKey)?;
        Ok(EcdhP256SharedSecret {
            bytes: SecretBytes::new(affine.x().to_bytes()),
        })
    }
}

impl KeyAgreement for EcdhP256 {
    type PrivateKey = EcdhP256PrivateKey;
    type PublicKey = EcdhP256PublicKey;
    type SharedSecret = EcdhP256SharedSecret;

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
        let private_key = EcdhP256PrivateKey::from_bytes([0x42; 32]).unwrap();
        let public_key = EcdhP256::public_key(&private_key);
        let shared = EcdhP256::agree(&private_key, &public_key).unwrap();
        assert_eq!(format!("{private_key:?}"), "EcdhP256PrivateKey([REDACTED])");
        assert_eq!(format!("{shared:?}"), "EcdhP256SharedSecret([REDACTED])");
        assert_eq!(public_key.as_bytes().len(), 65);
        assert_eq!(shared.expose_secret().len(), 32);
    }

    #[test]
    fn candidate_testing_adds_one_and_rejects_out_of_range_sources() {
        let mut one_source = FixedSource(0);
        let key = EcdhP256PrivateKey::generate(&mut one_source).unwrap();
        let mut one = [0_u8; 32];
        one[31] = 1;
        assert_eq!(key.scalar_bytes(), &one);

        let mut saturated = FixedSource(0xff);
        assert_eq!(
            EcdhP256PrivateKey::generate(&mut saturated).err(),
            Some(CryptoError::EntropyUnavailable)
        );
    }
}
