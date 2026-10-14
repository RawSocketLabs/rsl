//! PKCS #1 v1.5 encryption and signature encodings.
//!
//! > **RSAES-PKCS1-v1_5 and SHA-1 signatures are broken/deprecated choices. This readable RSA
//! > engine is also variable-time and unblinded. Do not use it to protect new data.**
//!
//! RFC 8017 preserves two different v1.5 encodings. They share a `00 || block-type` prefix but
//! solve different parsing problems and must never be interchanged:
//!
//! | Operation | Encoded message `EM`, exactly `k` bytes |
//! | --- | --- |
//! | encryption | `00 || 02 || PS(nonzero random bytes) || 00 || M` |
//! | signature | `00 || 01 || PS(FF bytes) || 00 || DigestInfo` |
//!
//! Here `k` is the modulus length in bytes and every padding string is at least eight bytes.
//! Encryption therefore accepts at most `k - 11` message bytes. Signature encoding includes the
//! algorithm's DER `DigestInfo` prefix as well as the digest; matching only a trailing digest is
//! incorrect.
//!
//! # Encryption walkthrough
//!
//! ```
//! use rsl_crypto_legacy::{
//!     RandomSource, Result,
//!     rsa::{RsaPublicKey, pkcs1v15::Pkcs1v15PublicOperations},
//! };
//!
//! struct NonzeroExampleSource(u8);
//! impl RandomSource for NonzeroExampleSource {
//!     fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
//!         for byte in output {
//!             self.0 = self.0.wrapping_add(1).max(1);
//!             *byte = self.0;
//!         }
//!         Ok(())
//!     }
//! }
//!
//! // Structurally valid didactic components, not a generated security key.
//! let public = RsaPublicKey::from_components(
//!     [0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
//!     [3],
//! )?;
//! let mut random = NonzeroExampleSource(0);
//! let ciphertext = public.encrypt_pkcs1v15(&mut random, b"x")?;
//! assert_eq!(ciphertext.as_bytes().len(), public.modulus_len());
//! # Ok::<(), rsl_crypto_legacy::CryptoError>(())
//! ```
//!
//! # Oracle warning
//!
//! [`Pkcs1v15PrivateOperations::decrypt_pkcs1v15`] maps every ciphertext/encoding defect to one
//! [`CryptoError::AuthenticationFailed`] value and scans
//! the whole decoded block for the separator. That API shape follows RFC 8017's warning against
//! distinguishable errors. It is **not sufficient oracle resistance**: input length, integer
//! arithmetic, branches, allocation, caller behavior, and the surrounding protocol can still
//! leak information. TLS-specific randomized premaster-secret handling belongs in a TLS package.
//!
//! # Signature scope
//!
//! Explicit SHA-1 and SHA-256 methods are exposed because an algorithm identifier is part of the
//! encoding. SHA-1 is broken for collision-sensitive signatures. The SHA-256 variant can reproduce
//! historical PKCS #1 v1.5 signatures but is isolated as legacy here; modern protocol policy must
//! make an explicit choice and should prefer modern signature designs.
//!
//! The exact source mapping and published-vector provenance are recorded in `STANDARDS.md` and
//! `tests/vectors/rsa-pkcs1v15/README.md`.

use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use zeroize::Zeroize;

use crate::{CryptoError, RandomSource, Result, SecurityClassification, SecurityStatus};
use rsl_crypto::digest::sha2::sha256::Sha256;

use super::{RsaPrivateKey, RsaPublicKey};
use crate::digest::sha1::Sha1;

/// RSAES-PKCS1-v1_5 lifecycle status: retained only to reproduce vulnerable historical profiles.
pub const RSAES_SECURITY_STATUS: SecurityStatus = SecurityStatus::Broken;

/// RSASSA-PKCS1-v1_5 with SHA-1 lifecycle status.
pub const RSASSA_SHA1_SECURITY_STATUS: SecurityStatus = SecurityStatus::Broken;

/// RSASSA-PKCS1-v1_5 with SHA-256 lifecycle status in this opt-in package.
pub const RSASSA_SHA256_SECURITY_STATUS: SecurityStatus = SecurityStatus::Legacy;

/// An RSAES-PKCS1-v1_5 ciphertext encoded as exactly one RSA modulus.
///
/// Ciphertext is public wire material, so this owner may be cloned and formatted. Construction
/// from bytes is intentionally key-neutral; decryption verifies the exact modulus length.
#[derive(Clone, Eq, Hash, PartialEq)]
#[must_use = "ciphertext must be serialized, stored, or passed to a decrypting peer"]
pub struct RsaPkcs1v15Ciphertext(Vec<u8>);

impl RsaPkcs1v15Ciphertext {
    /// Wrap bytes received from a wire codec without claiming that they are valid.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Borrow all encoded ciphertext bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consume the wrapper and return the encoded ciphertext bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl AsRef<[u8]> for RsaPkcs1v15Ciphertext {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl fmt::Debug for RsaPkcs1v15Ciphertext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RsaPkcs1v15Ciphertext")
            .field("length", &self.0.len())
            .finish_non_exhaustive()
    }
}

impl SecurityClassification for RsaPkcs1v15Ciphertext {
    const SECURITY_STATUS: SecurityStatus = RSAES_SECURITY_STATUS;
}

/// An RSASSA-PKCS1-v1_5 signature encoded as exactly one RSA modulus.
///
/// A wire parser may construct this owner before it knows which public key will verify the
/// signature. Verification performs the exact length, RSA, and encoding checks.
#[derive(Clone, Eq, Hash, PartialEq)]
#[must_use = "a signature should be serialized, stored, or verified"]
pub struct RsaPkcs1v15Signature(Vec<u8>);

impl RsaPkcs1v15Signature {
    /// Wrap untrusted signature bytes without claiming that they verify.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Borrow all encoded signature bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consume the wrapper and return the encoded signature bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl AsRef<[u8]> for RsaPkcs1v15Signature {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl fmt::Debug for RsaPkcs1v15Signature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RsaPkcs1v15Signature")
            .field("length", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Historical RSAES-PKCS1-v1_5 encryption and RSASSA-PKCS1-v1_5 verification on the shared
/// [`RsaPublicKey`].
///
/// The key type lives in `rsl_crypto::rsa`; this extension trait keeps the historical encodings
/// inside the opt-in package. Import it to call these methods.
pub trait Pkcs1v15PublicOperations {
    /// Encrypt a message with RFC 8017 §7.2 RSAES-PKCS1-v1_5.
    ///
    /// # Errors
    ///
    /// See [`RsaPublicKey`]'s implementation.
    fn encrypt_pkcs1v15<R: RandomSource>(
        &self,
        random: &mut R,
        message: impl AsRef<[u8]>,
    ) -> Result<RsaPkcs1v15Ciphertext>;

    /// Verify RSASSA-PKCS1-v1_5 with SHA-1 and exact RFC 8017 `DigestInfo` encoding.
    ///
    /// # Errors
    ///
    /// See [`RsaPublicKey`]'s implementation.
    fn verify_pkcs1v15_sha1(
        &self,
        message: impl AsRef<[u8]>,
        signature: &RsaPkcs1v15Signature,
    ) -> Result<()>;

    /// Verify RSASSA-PKCS1-v1_5 with SHA-256 and exact RFC 8017 `DigestInfo` encoding.
    ///
    /// # Errors
    ///
    /// See [`RsaPublicKey`]'s implementation.
    fn verify_pkcs1v15_sha256(
        &self,
        message: impl AsRef<[u8]>,
        signature: &RsaPkcs1v15Signature,
    ) -> Result<()>;
}

/// Historical RSAES-PKCS1-v1_5 decryption and RSASSA-PKCS1-v1_5 signing on the shared
/// [`RsaPrivateKey`].
///
/// Import it to call these methods. The private primitive is variable-time and unblinded.
pub trait Pkcs1v15PrivateOperations {
    /// Decrypt RFC 8017 §7.2 RSAES-PKCS1-v1_5 and return only the recovered message.
    ///
    /// # Errors
    ///
    /// See [`RsaPrivateKey`]'s implementation.
    fn decrypt_pkcs1v15(&self, ciphertext: &RsaPkcs1v15Ciphertext) -> Result<Vec<u8>>;

    /// Sign a message with RSASSA-PKCS1-v1_5 and SHA-1.
    ///
    /// # Errors
    ///
    /// See [`RsaPrivateKey`]'s implementation.
    fn sign_pkcs1v15_sha1(&self, message: impl AsRef<[u8]>) -> Result<RsaPkcs1v15Signature>;

    /// Sign a message with RSASSA-PKCS1-v1_5 and SHA-256.
    ///
    /// # Errors
    ///
    /// See [`RsaPrivateKey`]'s implementation.
    fn sign_pkcs1v15_sha256(&self, message: impl AsRef<[u8]>) -> Result<RsaPkcs1v15Signature>;
}

impl Pkcs1v15PublicOperations for RsaPublicKey {
    /// Encrypt a message with RFC 8017 §7.2 RSAES-PKCS1-v1_5.
    ///
    /// `random` fills the padding string. Zero bytes are rejected and resampled because zero is
    /// the delimiter. An integration crate must supply a cryptographically secure source; this
    /// package never silently selects one.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] when the message exceeds `k - 11`,
    /// [`CryptoError::InvalidKey`] when the modulus is too short for this encoding, or propagates
    /// an entropy-source failure. A source returning zero forever is eventually rejected with
    /// [`CryptoError::EntropyUnavailable`] instead of causing an infinite loop.
    fn encrypt_pkcs1v15<R: RandomSource>(
        &self,
        random: &mut R,
        message: impl AsRef<[u8]>,
    ) -> Result<RsaPkcs1v15Ciphertext> {
        let mut encoded = encode_encryption(self.modulus_len(), random, message.as_ref())?;
        let result = self.apply(&encoded).map(RsaPkcs1v15Ciphertext);
        encoded.zeroize();
        result
    }

    /// Verify RSASSA-PKCS1-v1_5 with SHA-1 and exact RFC 8017 `DigestInfo` encoding.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] only if SHA-1 cannot represent the message length,
    /// or [`CryptoError::InvalidSignature`] for every signature/key/encoding mismatch.
    fn verify_pkcs1v15_sha1(
        &self,
        message: impl AsRef<[u8]>,
        signature: &RsaPkcs1v15Signature,
    ) -> Result<()> {
        let digest = Sha1::digest(message)?;
        verify_encoded_signature(self, signature, &SHA1_DIGEST_INFO_PREFIX, digest.as_ref())
    }

    /// Verify RSASSA-PKCS1-v1_5 with SHA-256 and exact RFC 8017 `DigestInfo` encoding.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] only if SHA-256 cannot represent the message
    /// length, or [`CryptoError::InvalidSignature`] for every signature/key/encoding mismatch.
    fn verify_pkcs1v15_sha256(
        &self,
        message: impl AsRef<[u8]>,
        signature: &RsaPkcs1v15Signature,
    ) -> Result<()> {
        let digest = Sha256::digest(message)?;
        verify_encoded_signature(self, signature, &SHA256_DIGEST_INFO_PREFIX, digest.as_ref())
    }
}

fn verify_encoded_signature(
    key: &RsaPublicKey,
    signature: &RsaPkcs1v15Signature,
    digest_info_prefix: &[u8],
    digest: &[u8],
) -> Result<()> {
    let expected = encode_signature(key.modulus_len(), digest_info_prefix, digest)
        .map_err(|_| CryptoError::InvalidSignature)?;
    let actual = key
        .apply(signature.as_bytes())
        .map_err(|_| CryptoError::InvalidSignature)?;

    if bytes_equal(&actual, &expected) {
        Ok(())
    } else {
        Err(CryptoError::InvalidSignature)
    }
}

impl Pkcs1v15PrivateOperations for RsaPrivateKey {
    /// Decrypt RFC 8017 §7.2 RSAES-PKCS1-v1_5 and return only the recovered message.
    ///
    /// # Errors
    ///
    /// Every ciphertext length, representative, prefix, padding-string, and delimiter failure
    /// maps to [`CryptoError::AuthenticationFailed`]. See the module-level warning: a uniform enum
    /// is necessary but not sufficient protection against a padding oracle.
    fn decrypt_pkcs1v15(&self, ciphertext: &RsaPkcs1v15Ciphertext) -> Result<Vec<u8>> {
        if ciphertext.as_bytes().len() != self.modulus_len() {
            return Err(CryptoError::AuthenticationFailed);
        }

        let encoded = self
            .apply(ciphertext.as_bytes())
            .map_err(|_| CryptoError::AuthenticationFailed)?;
        decode_encryption(encoded)
    }

    /// Sign a message with RSASSA-PKCS1-v1_5 and SHA-1.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidKey`] if the modulus cannot contain the complete SHA-1
    /// `DigestInfo` and required padding, or [`CryptoError::MessageTooLong`] at SHA-1's length
    /// boundary. This method is variable-time and SHA-1 is broken for collision-resistant use.
    fn sign_pkcs1v15_sha1(&self, message: impl AsRef<[u8]>) -> Result<RsaPkcs1v15Signature> {
        let digest = Sha1::digest(message)?;
        sign_encoded(self, &SHA1_DIGEST_INFO_PREFIX, digest.as_ref())
    }

    /// Sign a message with RSASSA-PKCS1-v1_5 and SHA-256.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidKey`] if the modulus cannot contain the complete SHA-256
    /// `DigestInfo` and required padding, or [`CryptoError::MessageTooLong`] at SHA-256's length
    /// boundary. The private operation remains variable-time and unblinded.
    fn sign_pkcs1v15_sha256(&self, message: impl AsRef<[u8]>) -> Result<RsaPkcs1v15Signature> {
        let digest = Sha256::digest(message)?;
        sign_encoded(self, &SHA256_DIGEST_INFO_PREFIX, digest.as_ref())
    }
}

fn sign_encoded(key: &RsaPrivateKey, prefix: &[u8], digest: &[u8]) -> Result<RsaPkcs1v15Signature> {
    let mut encoded = encode_signature(key.modulus_len(), prefix, digest)?;
    let result = key.apply(&encoded).map(RsaPkcs1v15Signature);
    encoded.zeroize();
    result
}

// RFC 8017 Appendix B.1's DER DigestInfo encodings, through the OCTET STRING length byte.
const SHA1_DIGEST_INFO_PREFIX: [u8; 15] = [
    0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04, 0x14,
];
const SHA256_DIGEST_INFO_PREFIX: [u8; 19] = [
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
    0x00, 0x04, 0x20,
];
const MAX_ZERO_RETRIES: u16 = 256;

fn encode_encryption<R: RandomSource>(
    modulus_len: usize,
    random: &mut R,
    message: &[u8],
) -> Result<Vec<u8>> {
    let minimum_len = message
        .len()
        .checked_add(11)
        .ok_or(CryptoError::MessageTooLong)?;
    if minimum_len > modulus_len {
        return Err(if modulus_len < 11 {
            CryptoError::InvalidKey
        } else {
            CryptoError::MessageTooLong
        });
    }

    let padding_len = modulus_len - message.len() - 3;
    let mut encoded = vec![0_u8; modulus_len];
    encoded[1] = 2;
    random.fill_bytes(&mut encoded[2..2 + padding_len])?;

    for index in 2..2 + padding_len {
        let mut attempts = 0_u16;
        while encoded[index] == 0 {
            if attempts == MAX_ZERO_RETRIES {
                encoded.zeroize();
                return Err(CryptoError::EntropyUnavailable);
            }
            random.fill_bytes(core::slice::from_mut(&mut encoded[index]))?;
            attempts += 1;
        }
    }

    let delimiter = 2 + padding_len;
    encoded[delimiter] = 0;
    encoded[delimiter + 1..].copy_from_slice(message);
    Ok(encoded)
}

fn decode_encryption(mut encoded: Vec<u8>) -> Result<Vec<u8>> {
    let mut separator = 0_usize;
    let mut found_separator = false;

    for (index, byte) in encoded.iter().copied().enumerate().skip(2) {
        if !found_separator && byte == 0 {
            separator = index;
            found_separator = true;
        }
    }

    let valid = encoded.len() >= 11
        && encoded.first() == Some(&0)
        && encoded.get(1) == Some(&2)
        && found_separator
        && separator >= 10;

    if !valid {
        encoded.zeroize();
        return Err(CryptoError::AuthenticationFailed);
    }

    let message = encoded[separator + 1..].to_vec();
    encoded.zeroize();
    Ok(message)
}

fn encode_signature(modulus_len: usize, prefix: &[u8], digest: &[u8]) -> Result<Vec<u8>> {
    let digest_info_len = prefix
        .len()
        .checked_add(digest.len())
        .ok_or(CryptoError::InvalidKey)?;
    let minimum_len = digest_info_len
        .checked_add(11)
        .ok_or(CryptoError::InvalidKey)?;
    if modulus_len < minimum_len {
        return Err(CryptoError::InvalidKey);
    }

    let padding_len = modulus_len - digest_info_len - 3;
    let mut encoded = vec![0xff; modulus_len];
    encoded[0] = 0;
    encoded[1] = 1;
    encoded[2 + padding_len] = 0;
    let digest_info_start = 3 + padding_len;
    encoded[digest_info_start..digest_info_start + prefix.len()].copy_from_slice(prefix);
    encoded[digest_info_start + prefix.len()..].copy_from_slice(digest);
    Ok(encoded)
}

fn bytes_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[cfg(test)]
mod unit {
    use super::*;

    struct ZeroSource;

    impl RandomSource for ZeroSource {
        fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
            output.fill(0);
            Ok(())
        }
    }

    #[test]
    fn encryption_encoding_places_nonzero_padding_and_delimiter() {
        struct Incrementing(u8);
        impl RandomSource for Incrementing {
            fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
                for byte in output {
                    *byte = self.0;
                    self.0 = self.0.wrapping_add(1);
                }
                Ok(())
            }
        }

        let mut source = Incrementing(0);
        let encoded = encode_encryption(16, &mut source, b"abc").expect("three bytes fit");
        assert_eq!(&encoded[..2], &[0, 2]);
        assert!(encoded[2..12].iter().all(|byte| *byte != 0));
        assert_eq!(encoded[12], 0);
        assert_eq!(&encoded[13..], b"abc");
    }

    #[test]
    fn all_padding_errors_collapse_to_authentication_failed() {
        for malformed in [
            vec![],
            vec![0; 11],
            vec![0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0],
            vec![0, 2, 1, 1, 1, 1, 1, 1, 1, 0, 7],
            vec![0, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        ] {
            assert_eq!(
                decode_encryption(malformed),
                Err(CryptoError::AuthenticationFailed)
            );
        }
    }

    #[test]
    fn a_permanently_zero_source_is_bounded() {
        assert_eq!(
            encode_encryption(12, &mut ZeroSource, b"x"),
            Err(CryptoError::EntropyUnavailable),
        );
    }

    #[test]
    fn signature_encoding_contains_complete_digest_info() {
        let encoded = encode_signature(64, &SHA256_DIGEST_INFO_PREFIX, &[0x42; 32])
            .expect("a 512-bit modulus has room");
        assert_eq!(&encoded[..2], &[0, 1]);
        assert_eq!(&encoded[2..12], &[0xff; 10]);
        assert_eq!(encoded[12], 0);
        assert_eq!(&encoded[13..32], &SHA256_DIGEST_INFO_PREFIX);
        assert_eq!(&encoded[32..], &[0x42; 32]);
    }
}
