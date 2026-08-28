//! Typed Ed25519 key, signature, signing, and verification boundary.
//!
//! RFC 8032 §§5.1.5–5.1.7 define deterministic key expansion, signing, and verification. This
//! API additionally rejects small-order public keys and `R` points and checks the sufficient
//! uncofactored equation `[S]B = R + [k]A`; this is the strict behavior used by the differential
//! oracle and avoids accepting the well-known identity-point forgeries permitted by a bare
//! cofactored equation.

use core::fmt;
use zeroize::Zeroize;

use super::{point::EdwardsPoint, scalar::Scalar};
use crate::{
    CryptoError, Result, SecretBytes,
    digest::sha2::sha512::Sha512,
    random::RandomSource,
    signature::{Signer, Verifier},
};

const KEY_LEN: usize = 32;
const SIGNATURE_LEN: usize = 64;

/// A 32-byte Ed25519 private seed.
///
/// RFC 8032 hashes this seed with SHA-512 before each public-key or signing operation. The owner
/// is non-`Clone`, redacted, and zeroized on drop. It is a seed, not the expanded scalar and prefix.
/// See the [`Ed25519` teaching page](crate::signature::ed25519) for a published signing example.
pub struct Ed25519SigningKey {
    seed: SecretBytes<KEY_LEN>,
}

impl Ed25519SigningKey {
    /// Encoded private-seed size.
    pub const LEN: usize = KEY_LEN;

    /// Take ownership of an exact-size private seed.
    #[must_use]
    pub fn from_seed(seed: [u8; KEY_LEN]) -> Self {
        Self {
            seed: SecretBytes::new(seed),
        }
    }

    /// Generate a private seed using the caller-selected entropy source.
    ///
    /// # Errors
    ///
    /// Returns the source's error and clears the partially filled temporary when entropy fails.
    pub fn generate<R: RandomSource>(random: &mut R) -> Result<Self> {
        let mut seed = [0_u8; KEY_LEN];
        if let Err(error) = random.fill_bytes(&mut seed) {
            seed.zeroize();
            return Err(error);
        }
        Ok(Self::from_seed(seed))
    }

    /// Derive the canonical public verification key.
    #[must_use]
    pub fn verifying_key(&self) -> Ed25519VerifyingKey {
        let mut hash = Sha512::new();
        let _infallible_for_fixed_seed = hash.update(self.seed.expose_secret());
        let mut expanded = hash.finalize().into_bytes();
        let mut scalar = [0_u8; 32];
        scalar.copy_from_slice(&expanded[..32]);
        prepare_secret_scalar(&mut scalar);
        let bytes = EdwardsPoint::basepoint().multiply(&scalar).compress();
        scalar.zeroize();
        expanded.zeroize();
        Ed25519VerifyingKey { bytes }
    }

    /// Deterministically sign one complete message as pure Ed25519.
    ///
    /// RFC 8032 derives the nonce from the secret prefix and message; this operation consumes no
    /// external randomness. The message is signed exactly as supplied, with no protocol framing,
    /// context string, or implicit prehash.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] if SHA-512 cannot represent an input length.
    pub fn sign(&self, message: impl AsRef<[u8]>) -> Result<Ed25519Signature> {
        self.sign_bytes(message.as_ref())
    }

    fn sign_bytes(&self, message: &[u8]) -> Result<Ed25519Signature> {
        let mut expanded = Sha512::digest(self.seed.expose_secret())?.into_bytes();
        let mut secret_scalar_bytes = [0_u8; 32];
        secret_scalar_bytes.copy_from_slice(&expanded[..32]);
        prepare_secret_scalar(&mut secret_scalar_bytes);

        let public_bytes = EdwardsPoint::basepoint()
            .multiply(&secret_scalar_bytes)
            .compress();
        let nonce = hash_to_scalar(&[&expanded[32..], message])?;
        let encoded_r = EdwardsPoint::basepoint()
            .multiply(&nonce.to_bytes())
            .compress();
        let challenge = hash_to_scalar(&[&encoded_r, &public_bytes, message])?;
        let secret_scalar = Scalar::reduce_32(&secret_scalar_bytes);
        let s = nonce.add(&challenge.multiply(&secret_scalar));

        let mut signature = [0_u8; SIGNATURE_LEN];
        signature[..32].copy_from_slice(&encoded_r);
        signature[32..].copy_from_slice(&s.to_bytes());
        secret_scalar_bytes.zeroize();
        expanded.zeroize();
        Ok(Ed25519Signature(signature))
    }
}

impl fmt::Debug for Ed25519SigningKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Ed25519SigningKey([REDACTED])")
    }
}

/// A validated canonical Ed25519 public key.
///
/// Construction performs point decoding immediately so malformed wire values cannot inhabit this
/// type. See [`Ed25519VerifyingKey::verify`] and the module's published worked example.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Ed25519VerifyingKey {
    bytes: [u8; KEY_LEN],
}

impl Ed25519VerifyingKey {
    /// Encoded public-key size.
    pub const LEN: usize = KEY_LEN;

    /// Parse a canonical curve point and reject small-order public keys.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] when point decoding fails or the point has order
    /// dividing eight.
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Result<Self> {
        let point = EdwardsPoint::decompress(&bytes).ok_or(CryptoError::InvalidPublicKey)?;
        if point.is_small_order() {
            return Err(CryptoError::InvalidPublicKey);
        }
        Ok(Self { bytes })
    }

    /// Borrow the canonical wire encoding.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.bytes
    }

    /// Consume the key into its canonical wire encoding.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; KEY_LEN] {
        self.bytes
    }

    /// Strictly verify a pure Ed25519 signature over the exact message bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidSignature`] for malformed `R`, non-canonical `S`, a
    /// small-order `R`, or a failed group equation. Excessive SHA-512 message length is returned
    /// as [`CryptoError::MessageTooLong`].
    pub fn verify(&self, message: impl AsRef<[u8]>, signature: &Ed25519Signature) -> Result<()> {
        self.verify_bytes(message.as_ref(), signature)
    }

    fn verify_bytes(&self, message: &[u8], signature: &Ed25519Signature) -> Result<()> {
        let public_point = EdwardsPoint::decompress(&self.bytes)
            .expect("validated verifying-key bytes remain a valid point");
        let (encoded_r, encoded_s) = signature.parts();
        let r = EdwardsPoint::decompress(encoded_r).ok_or(CryptoError::InvalidSignature)?;
        if r.is_small_order() {
            return Err(CryptoError::InvalidSignature);
        }
        let s = Scalar::from_canonical_bytes(encoded_s).ok_or(CryptoError::InvalidSignature)?;
        let challenge = hash_to_scalar(&[encoded_r, &self.bytes, message])?;
        let left = EdwardsPoint::basepoint().multiply(&s.to_bytes());
        let right = r.add(&public_point.multiply(&challenge.to_bytes()));

        if left.equals(&right) {
            Ok(())
        } else {
            Err(CryptoError::InvalidSignature)
        }
    }
}

impl AsRef<[u8]> for Ed25519VerifyingKey {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl TryFrom<&[u8]> for Ed25519VerifyingKey {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact = <[u8; KEY_LEN]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
            name: "Ed25519 public key",
            expected: KEY_LEN,
            actual,
        })?;
        Self::from_bytes(exact)
    }
}

/// A detached 64-byte Ed25519 signature `ENC(R) || ENC(S)`.
///
/// Construction from an array preserves received bytes. Structural point/scalar checks occur at
/// verification so parsing a detached wire field does not require its public key or message.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Ed25519Signature([u8; SIGNATURE_LEN]);

impl Ed25519Signature {
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
        let r = self.0[..32]
            .try_into()
            .expect("first signature half is 32 bytes");
        let s = self.0[32..]
            .try_into()
            .expect("second signature half is 32 bytes");
        (r, s)
    }
}

impl AsRef<[u8]> for Ed25519Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for Ed25519Signature {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; SIGNATURE_LEN]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "Ed25519 signature",
                expected: SIGNATURE_LEN,
                actual,
            })?;
        Ok(Self(exact))
    }
}

impl Signer for Ed25519SigningKey {
    type Signature = Ed25519Signature;

    fn sign<R: RandomSource>(&self, _random: &mut R, message: &[u8]) -> Result<Self::Signature> {
        self.sign_bytes(message)
    }
}

impl Verifier<Ed25519Signature> for Ed25519VerifyingKey {
    fn verify(&self, message: &[u8], signature: &Ed25519Signature) -> Result<()> {
        self.verify_bytes(message, signature)
    }
}

/// Apply RFC 8032 §5.1.5's prune operation to the lower half of `SHA-512(seed)`.
fn prepare_secret_scalar(bytes: &mut [u8; 32]) {
    bytes[0] &= 0xf8;
    bytes[31] &= 0x3f;
    bytes[31] |= 0x40;
}

/// Hash concatenated byte strings and reduce the 512-bit little-endian result modulo `L`.
fn hash_to_scalar(parts: &[&[u8]]) -> Result<Scalar> {
    let mut hash = Sha512::new();
    for part in parts {
        hash.update(part)?;
    }
    let mut digest = hash.finalize().into_bytes();
    let scalar = Scalar::reduce_wide(&digest);
    digest.zeroize();
    Ok(scalar)
}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::format;

    #[test]
    fn secret_owner_is_redacted_and_public_values_have_exact_sizes() {
        let signing = Ed25519SigningKey::from_seed([0x42; 32]);
        assert_eq!(format!("{signing:?}"), "Ed25519SigningKey([REDACTED])");
        assert_eq!(signing.verifying_key().as_bytes().len(), 32);
        assert_eq!(signing.sign(b"message").unwrap().as_bytes().len(), 64);
    }

    #[test]
    fn identity_public_key_and_identity_r_are_rejected() {
        let mut identity = [0_u8; 32];
        identity[0] = 1;
        assert_eq!(
            Ed25519VerifyingKey::from_bytes(identity),
            Err(CryptoError::InvalidPublicKey)
        );

        let signing = Ed25519SigningKey::from_seed([0x42; 32]);
        let verifying = signing.verifying_key();
        let mut forged = [0_u8; 64];
        forged[0] = 1;
        assert_eq!(
            verifying.verify(b"anything", &Ed25519Signature::from_bytes(forged)),
            Err(CryptoError::InvalidSignature)
        );
    }
}
