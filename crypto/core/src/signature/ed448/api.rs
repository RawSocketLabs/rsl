//! Typed Ed448 key, signature, context, signing, and verification boundary.
//!
//! RFC 8032 §§5.2.5–5.2.7 define key expansion, signing, and verification with
//! `H = SHAKE256(·, 114)` and the prefix `dom4(F, C)`, which — unlike Ed25519's `dom2` — is
//! always present: pure Ed448 uses `dom4(0, C)` with an empty default context. This API
//! additionally rejects small-order public keys and `R` points and checks the sufficient
//! uncofactored equation `[S]B = R + [k]A`, matching the strict Ed25519 policy.

use alloc::vec::Vec;
use core::fmt;
use zeroize::Zeroize;

use super::{point::EdwardsPoint, scalar::Scalar};
use crate::{
    CryptoError, Result, SecretBytes,
    digest::sha3::Shake256,
    random::RandomSource,
    signature::{Signer, Verifier},
};

const KEY_LEN: usize = 57;
const SIGNATURE_LEN: usize = 114;
const HASH_LEN: usize = 114;
const FLAG_PURE: u8 = 0;
const FLAG_PREHASH: u8 = 1;
/// RFC 8032 §5.2 `dom4` prefix.
const DOM4_PREFIX: &[u8] = b"SigEd448";

/// A validated RFC 8032 context string of at most 255 bytes.
///
/// Ed448 always carries a context; the default is the empty string, which [`None`] selects in
/// the signing and verification methods.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Ed448Context {
    bytes: Vec<u8>,
}

impl Ed448Context {
    /// Largest context length `octet(OLEN(C))` can encode.
    pub const MAX_LEN: usize = 255;

    /// Validate a context of 1 to 255 bytes (use `None` for the empty default).
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] for an empty context or one longer than 255 bytes.
    pub fn new(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            return Err(CryptoError::InvalidLength {
                name: "Ed448 context",
                expected: 1,
                actual: 0,
            });
        }
        if bytes.len() > Self::MAX_LEN {
            return Err(CryptoError::InvalidLength {
                name: "Ed448 context",
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

/// RFC 8032 §5.2 `dom4(F, C) = "SigEd448" || octet(F) || octet(OLEN(C)) || C`.
fn dom4(flag: u8, context: &[u8]) -> Vec<u8> {
    let mut dom = Vec::with_capacity(DOM4_PREFIX.len() + 2 + context.len());
    dom.extend_from_slice(DOM4_PREFIX);
    dom.push(flag);
    dom.push(u8::try_from(context.len()).expect("a validated context has at most 255 bytes"));
    dom.extend_from_slice(context);
    dom
}

fn context_bytes(context: Option<&Ed448Context>) -> &[u8] {
    context.map_or(&[][..], Ed448Context::as_bytes)
}

/// A 57-byte Ed448 private key.
///
/// RFC 8032 hashes it with `SHAKE256(·, 114)` before every operation. Non-`Clone`, redacted,
/// and zeroized on drop.
pub struct Ed448SigningKey {
    seed: SecretBytes<KEY_LEN>,
}

impl Ed448SigningKey {
    /// Encoded private-key size.
    pub const LEN: usize = KEY_LEN;

    /// Take ownership of an exact-size private key.
    #[must_use]
    pub fn from_seed(seed: [u8; KEY_LEN]) -> Self {
        Self {
            seed: SecretBytes::new(seed),
        }
    }

    /// Generate a private key using the caller-selected entropy source.
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

    /// §5.2.5: derive the public key `A = [s]B`.
    #[must_use]
    pub fn verifying_key(&self) -> Ed448VerifyingKey {
        let mut expanded = self.expand();
        let mut scalar = [0_u8; KEY_LEN];
        scalar.copy_from_slice(&expanded[..KEY_LEN]);
        prepare_secret_scalar(&mut scalar);
        let bytes = EdwardsPoint::basepoint().multiply(&scalar).compress();
        scalar.zeroize();
        expanded.zeroize();
        Ed448VerifyingKey { bytes }
    }

    /// §5.2.6 pure Ed448 with the empty default context or an explicit one.
    ///
    /// # Errors
    ///
    /// Never fails; the `Result` keeps the signature uniform with the other schemes.
    pub fn sign(
        &self,
        context: Option<&Ed448Context>,
        message: impl AsRef<[u8]>,
    ) -> Result<Ed448Signature> {
        Ok(self.sign_domain(&dom4(FLAG_PURE, context_bytes(context)), message.as_ref()))
    }

    /// §5.2 Ed448ph: sign `PH(M) = SHAKE256(M, 64)` computed by the caller.
    ///
    /// # Errors
    ///
    /// Never fails; see [`Self::sign`].
    pub fn sign_prehashed(
        &self,
        prehashed_message: &[u8; 64],
        context: Option<&Ed448Context>,
    ) -> Result<Ed448Signature> {
        Ok(self.sign_domain(
            &dom4(FLAG_PREHASH, context_bytes(context)),
            prehashed_message,
        ))
    }

    /// `SHAKE256(private key, 114)`.
    fn expand(&self) -> [u8; HASH_LEN] {
        let mut expanded = [0_u8; HASH_LEN];
        Shake256::digest_into(self.seed.expose_secret(), &mut expanded);
        expanded
    }

    /// §5.2.6 steps 1–6 with `dom4` prefixed to both hash inputs.
    fn sign_domain(&self, dom: &[u8], message: &[u8]) -> Ed448Signature {
        let mut expanded = self.expand();
        let mut secret_scalar_bytes = [0_u8; KEY_LEN];
        secret_scalar_bytes.copy_from_slice(&expanded[..KEY_LEN]);
        prepare_secret_scalar(&mut secret_scalar_bytes);

        let public_bytes = EdwardsPoint::basepoint()
            .multiply(&secret_scalar_bytes)
            .compress();
        let nonce = hash_to_scalar(&[dom, &expanded[KEY_LEN..], message]);
        let encoded_r = EdwardsPoint::basepoint()
            .multiply(&nonce.to_bytes())
            .compress();
        let challenge = hash_to_scalar(&[dom, &encoded_r, &public_bytes, message]);
        let secret_scalar = Scalar::reduce_57(&secret_scalar_bytes);
        let s = nonce.add(&challenge.multiply(&secret_scalar));

        let mut signature = [0_u8; SIGNATURE_LEN];
        signature[..KEY_LEN].copy_from_slice(&encoded_r);
        signature[KEY_LEN..].copy_from_slice(&s.to_bytes());
        secret_scalar_bytes.zeroize();
        expanded.zeroize();
        Ed448Signature(signature)
    }
}

impl fmt::Debug for Ed448SigningKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Ed448SigningKey([REDACTED])")
    }
}

/// A validated canonical Ed448 public key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Ed448VerifyingKey {
    bytes: [u8; KEY_LEN],
}

impl Ed448VerifyingKey {
    /// Encoded public-key size.
    pub const LEN: usize = KEY_LEN;

    /// Parse a canonical curve point and reject small-order public keys.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] when decoding fails or the point has order
    /// dividing four.
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

    /// §5.2.7 strict verification of pure Ed448 under the given (or empty) context.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidSignature`] for malformed `R`, non-canonical `S`, a
    /// small-order `R`, or a failed group equation.
    pub fn verify(
        &self,
        context: Option<&Ed448Context>,
        message: impl AsRef<[u8]>,
        signature: &Ed448Signature,
    ) -> Result<()> {
        self.verify_domain(
            &dom4(FLAG_PURE, context_bytes(context)),
            message.as_ref(),
            signature,
        )
    }

    /// Ed448ph verification over a caller-computed `SHAKE256(M, 64)`.
    ///
    /// # Errors
    ///
    /// As for [`Self::verify`].
    pub fn verify_prehashed(
        &self,
        prehashed_message: &[u8; 64],
        context: Option<&Ed448Context>,
        signature: &Ed448Signature,
    ) -> Result<()> {
        self.verify_domain(
            &dom4(FLAG_PREHASH, context_bytes(context)),
            prehashed_message,
            signature,
        )
    }

    fn verify_domain(&self, dom: &[u8], message: &[u8], signature: &Ed448Signature) -> Result<()> {
        let public_point = EdwardsPoint::decompress(&self.bytes)
            .expect("validated verifying-key bytes remain a valid point");
        let (encoded_r, encoded_s) = signature.parts();
        let r = EdwardsPoint::decompress(encoded_r).ok_or(CryptoError::InvalidSignature)?;
        if r.is_small_order() {
            return Err(CryptoError::InvalidSignature);
        }
        let s = Scalar::from_canonical_bytes(encoded_s).ok_or(CryptoError::InvalidSignature)?;
        let challenge = hash_to_scalar(&[dom, encoded_r, &self.bytes, message]);
        let left = EdwardsPoint::basepoint().multiply(&s.to_bytes());
        let right = r.add(&public_point.multiply(&challenge.to_bytes()));
        if left.equals(&right) {
            Ok(())
        } else {
            Err(CryptoError::InvalidSignature)
        }
    }
}

impl AsRef<[u8]> for Ed448VerifyingKey {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl TryFrom<&[u8]> for Ed448VerifyingKey {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact = <[u8; KEY_LEN]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
            name: "Ed448 public key",
            expected: KEY_LEN,
            actual,
        })?;
        Self::from_bytes(exact)
    }
}

/// A detached 114-byte Ed448 signature `ENC(R) || ENC(S)`.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct Ed448Signature([u8; SIGNATURE_LEN]);

impl Ed448Signature {
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

    fn parts(&self) -> (&[u8; KEY_LEN], &[u8; KEY_LEN]) {
        let r = self.0[..KEY_LEN].try_into().expect("R is 57 bytes");
        let s = self.0[KEY_LEN..].try_into().expect("S is 57 bytes");
        (r, s)
    }
}

impl fmt::Debug for Ed448Signature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Ed448Signature(")?;
        for byte in &self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

impl AsRef<[u8]> for Ed448Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for Ed448Signature {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; SIGNATURE_LEN]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "Ed448 signature",
                expected: SIGNATURE_LEN,
                actual,
            })?;
        Ok(Self(exact))
    }
}

impl Signer for Ed448SigningKey {
    type Signature = Ed448Signature;

    /// Pure Ed448 with the empty context; deterministic, so `random` is ignored.
    fn sign<R: RandomSource>(&self, _random: &mut R, message: &[u8]) -> Result<Self::Signature> {
        self.sign(None, message)
    }
}

impl Verifier<Ed448Signature> for Ed448VerifyingKey {
    fn verify(&self, message: &[u8], signature: &Ed448Signature) -> Result<()> {
        self.verify(None, message, signature)
    }
}

/// §5.2.5 step 2: clear the two low bits of byte 0, clear byte 56, set the top bit of byte 55.
fn prepare_secret_scalar(bytes: &mut [u8; KEY_LEN]) {
    bytes[0] &= 0xfc;
    bytes[56] = 0;
    bytes[55] |= 0x80;
}

/// `SHAKE256(parts…, 114)` interpreted little-endian and reduced modulo `L`.
fn hash_to_scalar(parts: &[&[u8]]) -> Scalar {
    let mut xof = Shake256::new();
    for part in parts {
        xof.update(part);
    }
    let mut digest = [0_u8; HASH_LEN];
    xof.squeeze(&mut digest);
    let scalar = Scalar::reduce_wide(&digest);
    digest.zeroize();
    scalar
}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::format;

    #[test]
    fn dom4_layout_and_variant_separation() {
        let dom = dom4(FLAG_PURE, b"foo");
        assert_eq!(&dom[..8], b"SigEd448");
        assert_eq!(&dom[8..], &[0x00, 0x03, b'f', b'o', b'o']);
        let signing = Ed448SigningKey::from_seed([0x42; 57]);
        let verifying = signing.verifying_key();
        let context = Ed448Context::new(b"foo").unwrap();
        let pure = signing.sign(None, b"m").unwrap();
        let ctx = signing.sign(Some(&context), b"m").unwrap();
        assert_ne!(pure, ctx);
        assert!(verifying.verify(None, b"m", &pure).is_ok());
        assert!(verifying.verify(Some(&context), b"m", &ctx).is_ok());
        assert!(verifying.verify(None, b"m", &ctx).is_err());
        assert_eq!(format!("{signing:?}"), "Ed448SigningKey([REDACTED])");
    }

    #[test]
    fn identity_public_key_and_identity_r_are_rejected() {
        let mut identity = [0_u8; 57];
        identity[0] = 1;
        assert_eq!(
            Ed448VerifyingKey::from_bytes(identity),
            Err(CryptoError::InvalidPublicKey)
        );
        let verifying = Ed448SigningKey::from_seed([0x42; 57]).verifying_key();
        let mut forged = [0_u8; 114];
        forged[0] = 1;
        assert_eq!(
            verifying.verify(None, b"anything", &Ed448Signature::from_bytes(forged)),
            Err(CryptoError::InvalidSignature)
        );
    }
}
