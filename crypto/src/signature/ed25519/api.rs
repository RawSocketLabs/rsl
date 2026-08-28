//! Typed Ed25519 key, signature, signing, and verification boundary.
//!
//! RFC 8032 §§5.1.5–5.1.7 define deterministic key expansion, signing, and verification. This
//! API additionally rejects small-order public keys and `R` points and checks the sufficient
//! uncofactored equation `[S]B = R + [k]A`; this is the strict behavior used by the differential
//! oracle and avoids accepting the well-known identity-point forgeries permitted by a bare
//! cofactored equation.

use alloc::vec::Vec;
use core::fmt;
use zeroize::Zeroize;

use super::{point::EdwardsPoint, scalar::Scalar};
use crate::{
    CryptoError, Result, SecretBytes,
    digest::sha2::sha512::{Sha512, Sha512Digest},
    random::RandomSource,
    signature::{Signer, Verifier},
};

const KEY_LEN: usize = 32;
const SIGNATURE_LEN: usize = 64;

/// RFC 8032 §5.1 `dom2` flag values: `0` for Ed25519ctx, `1` for Ed25519ph.
const FLAG_CONTEXT: u8 = 0;
const FLAG_PREHASH: u8 = 1;

/// RFC 8032 §5.1 domain-separation prefix that precedes the flag, context length, and context.
const DOM2_PREFIX: &[u8] = b"SigEd25519 no Ed25519 collisions";

/// A validated RFC 8032 context string of 1 to 255 bytes for Ed25519ctx and Ed25519ph.
///
/// Pure Ed25519 has no context. Ed25519ctx requires a non-empty context; Ed25519ph accepts an
/// optional one. The bytes are public domain-separation data, so the owner is `Clone`.
///
/// # Examples
///
/// ```
/// use rsl_crypto::{CryptoError, signature::ed25519::Ed25519Context};
///
/// assert!(Ed25519Context::new(b"foo").is_ok());
/// assert_eq!(Ed25519Context::new(b"").err(), Some(CryptoError::InvalidLength {
///     name: "Ed25519 context",
///     expected: 1,
///     actual: 0,
/// }));
/// assert!(Ed25519Context::new(&[0; 256]).is_err());
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Ed25519Context {
    bytes: Vec<u8>,
}

impl Ed25519Context {
    /// Largest context length `octet(OLEN(C))` can encode.
    pub const MAX_LEN: usize = 255;

    /// Validate a context of 1 to 255 bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] for an empty context (`expected: 1`) or one longer
    /// than 255 bytes (`expected: 255`).
    pub fn new(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            return Err(CryptoError::InvalidLength {
                name: "Ed25519 context",
                expected: 1,
                actual: 0,
            });
        }
        if bytes.len() > Self::MAX_LEN {
            return Err(CryptoError::InvalidLength {
                name: "Ed25519 context",
                expected: Self::MAX_LEN,
                actual: bytes.len(),
            });
        }
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    /// Borrow the context bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// RFC 8032 §5.1 `dom2(F, C)` for a variant; pure Ed25519 uses the empty string instead.
fn dom2(flag: u8, context: &[u8]) -> Vec<u8> {
    let mut dom = Vec::with_capacity(DOM2_PREFIX.len() + 2 + context.len());
    dom.extend_from_slice(DOM2_PREFIX);
    dom.push(flag);
    dom.push(u8::try_from(context.len()).expect("a validated context has at most 255 bytes"));
    dom.extend_from_slice(context);
    dom
}

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

    /// Sign one complete message as Ed25519ctx with a mandatory context (RFC 8032 §5.1, `F = 0`).
    ///
    /// The context separates signatures made for different purposes under one key. It is not
    /// secret and must be known exactly by the verifier.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] if SHA-512 cannot represent an input length.
    pub fn sign_with_context(
        &self,
        context: &Ed25519Context,
        message: impl AsRef<[u8]>,
    ) -> Result<Ed25519Signature> {
        self.sign_domain(&dom2(FLAG_CONTEXT, context.as_bytes()), message.as_ref())
    }

    /// Sign a SHA-512 digest of the message as Ed25519ph (RFC 8032 §5.1, `F = 1`).
    ///
    /// `PH(M) = SHA-512(M)` is computed by the caller, which allows streaming the message. The
    /// optional context defaults to the empty string as the RFC specifies.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] only at SHA-512's internal length boundary, which
    /// the fixed-size inputs cannot reach.
    pub fn sign_prehashed(
        &self,
        prehashed_message: &Sha512Digest,
        context: Option<&Ed25519Context>,
    ) -> Result<Ed25519Signature> {
        let context = context.map_or(&[][..], Ed25519Context::as_bytes);
        self.sign_domain(&dom2(FLAG_PREHASH, context), prehashed_message.as_bytes())
    }

    fn sign_bytes(&self, message: &[u8]) -> Result<Ed25519Signature> {
        self.sign_domain(&[], message)
    }

    /// RFC 8032 §5.1.6 with `dom2` prefixed to both hash inputs; `dom` is empty for pure Ed25519.
    fn sign_domain(&self, dom: &[u8], message: &[u8]) -> Result<Ed25519Signature> {
        let mut expanded = Sha512::digest(self.seed.expose_secret())?.into_bytes();
        let mut secret_scalar_bytes = [0_u8; 32];
        secret_scalar_bytes.copy_from_slice(&expanded[..32]);
        prepare_secret_scalar(&mut secret_scalar_bytes);

        let public_bytes = EdwardsPoint::basepoint()
            .multiply(&secret_scalar_bytes)
            .compress();
        let nonce = hash_to_scalar(&[dom, &expanded[32..], message])?;
        let encoded_r = EdwardsPoint::basepoint()
            .multiply(&nonce.to_bytes())
            .compress();
        let challenge = hash_to_scalar(&[dom, &encoded_r, &public_bytes, message])?;
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

    /// Strictly verify an Ed25519ctx signature under the exact context the signer used.
    ///
    /// # Errors
    ///
    /// As for [`Self::verify`].
    pub fn verify_with_context(
        &self,
        context: &Ed25519Context,
        message: impl AsRef<[u8]>,
        signature: &Ed25519Signature,
    ) -> Result<()> {
        self.verify_domain(
            &dom2(FLAG_CONTEXT, context.as_bytes()),
            message.as_ref(),
            signature,
        )
    }

    /// Strictly verify an Ed25519ph signature over a caller-computed SHA-512 digest.
    ///
    /// # Errors
    ///
    /// As for [`Self::verify`].
    pub fn verify_prehashed(
        &self,
        prehashed_message: &Sha512Digest,
        context: Option<&Ed25519Context>,
        signature: &Ed25519Signature,
    ) -> Result<()> {
        let context = context.map_or(&[][..], Ed25519Context::as_bytes);
        self.verify_domain(
            &dom2(FLAG_PREHASH, context),
            prehashed_message.as_bytes(),
            signature,
        )
    }

    fn verify_bytes(&self, message: &[u8], signature: &Ed25519Signature) -> Result<()> {
        self.verify_domain(&[], message, signature)
    }

    /// RFC 8032 §5.1.7 with `dom2` prefixed to the challenge hash; `dom` is empty for pure.
    fn verify_domain(
        &self,
        dom: &[u8],
        message: &[u8],
        signature: &Ed25519Signature,
    ) -> Result<()> {
        let public_point = EdwardsPoint::decompress(&self.bytes)
            .expect("validated verifying-key bytes remain a valid point");
        let (encoded_r, encoded_s) = signature.parts();
        let r = EdwardsPoint::decompress(encoded_r).ok_or(CryptoError::InvalidSignature)?;
        if r.is_small_order() {
            return Err(CryptoError::InvalidSignature);
        }
        let s = Scalar::from_canonical_bytes(encoded_s).ok_or(CryptoError::InvalidSignature)?;
        let challenge = hash_to_scalar(&[dom, encoded_r, &self.bytes, message])?;
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
    fn dom2_matches_the_rfc_layout_and_variants_are_domain_separated() {
        let dom = dom2(FLAG_CONTEXT, b"foo");
        assert_eq!(&dom[..32], b"SigEd25519 no Ed25519 collisions");
        assert_eq!(&dom[32..], &[0x00, 0x03, b'f', b'o', b'o']);

        let signing = Ed25519SigningKey::from_seed([0x42; 32]);
        let verifying = signing.verifying_key();
        let context = Ed25519Context::new(b"foo").unwrap();
        let pure = signing.sign(b"m").unwrap();
        let ctx = signing.sign_with_context(&context, b"m").unwrap();
        let digest = Sha512::digest(b"m").unwrap();
        let ph = signing.sign_prehashed(&digest, None).unwrap();
        assert_ne!(pure, ctx);
        assert_ne!(pure, ph);
        assert_ne!(ctx, ph);
        assert!(verifying.verify_with_context(&context, b"m", &ctx).is_ok());
        assert!(verifying.verify_prehashed(&digest, None, &ph).is_ok());
        assert!(verifying.verify(b"m", &ctx).is_err());
        assert!(
            verifying
                .verify_with_context(&context, b"m", &pure)
                .is_err()
        );
        assert!(
            verifying
                .verify_prehashed(&digest, Some(&context), &ph)
                .is_err()
        );
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
