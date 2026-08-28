//! Signing adapters joining canonical `rsl-x509` construction to `rsl-crypto` keys.
//!
//! These adapters select only the modern certificate signature identifiers supported by the
//! normal validator. They do not allocate serials, authorize names, choose lifetimes, or turn a
//! constructed certificate into trusted state. Callers retain those issuance-policy decisions.

use alloc::vec::Vec;
use core::fmt;

use rsl_crypto::{
    CryptoError,
    signature::{
        ecdsa_p256::EcdsaP256SigningKey, ecdsa_p384::EcdsaP384SigningKey, ed448::Ed448SigningKey,
        ed25519::Ed25519SigningKey,
    },
};
use rsl_x509::builder::{
    CertificateSigner, SignatureAlgorithmDer, SubjectPublicKeyInfoDer, ecdsa_signature_value,
};

/// Failure while a built-in certificate signer produces the signature value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IssuanceError {
    /// The cryptographic signing operation failed.
    Crypto(CryptoError),
    /// The algorithm-specific signature could not be framed as canonical X.509 syntax.
    Encoding(rsl_x509::Error),
}

impl fmt::Display for IssuanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Crypto(error) => write!(formatter, "certificate signing failed: {error}"),
            Self::Encoding(error) => {
                write!(formatter, "certificate signature encoding failed: {error}")
            }
        }
    }
}

impl core::error::Error for IssuanceError {}

impl From<CryptoError> for IssuanceError {
    fn from(error: CryptoError) -> Self {
        Self::Crypto(error)
    }
}

impl From<rsl_x509::Error> for IssuanceError {
    fn from(error: rsl_x509::Error) -> Self {
        Self::Encoding(error)
    }
}

/// Pure Ed25519 certificate signer.
#[derive(Debug)]
pub struct Ed25519CertificateSigner<'a> {
    key: &'a Ed25519SigningKey,
}

impl<'a> Ed25519CertificateSigner<'a> {
    /// Wraps a signing key without taking ownership of it.
    #[must_use]
    pub const fn new(key: &'a Ed25519SigningKey) -> Self {
        Self { key }
    }

    /// Constructs the matching Ed25519 subject-public-key encoding.
    ///
    /// # Errors
    ///
    /// Canonical DER construction fails.
    pub fn subject_public_key_info(&self) -> rsl_x509::Result<SubjectPublicKeyInfoDer> {
        SubjectPublicKeyInfoDer::ed25519(self.key.verifying_key().as_bytes())
    }
}

impl CertificateSigner for Ed25519CertificateSigner<'_> {
    type Error = IssuanceError;

    fn signature_algorithm(&self) -> rsl_x509::Result<SignatureAlgorithmDer> {
        SignatureAlgorithmDer::ed25519()
    }

    fn sign(&self, tbs_certificate: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Ok(self.key.sign(tbs_certificate)?.as_bytes().to_vec())
    }
}

/// Pure Ed448 certificate signer using RFC 8410's empty context.
#[derive(Debug)]
pub struct Ed448CertificateSigner<'a> {
    key: &'a Ed448SigningKey,
}

impl<'a> Ed448CertificateSigner<'a> {
    /// Wraps a signing key without taking ownership of it.
    #[must_use]
    pub const fn new(key: &'a Ed448SigningKey) -> Self {
        Self { key }
    }

    /// Constructs the matching Ed448 subject-public-key encoding.
    ///
    /// # Errors
    ///
    /// Canonical DER construction fails.
    pub fn subject_public_key_info(&self) -> rsl_x509::Result<SubjectPublicKeyInfoDer> {
        SubjectPublicKeyInfoDer::ed448(self.key.verifying_key().as_bytes())
    }
}

impl CertificateSigner for Ed448CertificateSigner<'_> {
    type Error = IssuanceError;

    fn signature_algorithm(&self) -> rsl_x509::Result<SignatureAlgorithmDer> {
        SignatureAlgorithmDer::ed448()
    }

    fn sign(&self, tbs_certificate: &[u8]) -> Result<Vec<u8>, Self::Error> {
        Ok(self.key.sign(None, tbs_certificate)?.as_bytes().to_vec())
    }
}

/// ECDSA P-256/SHA-256 certificate signer with deterministic RFC 6979 nonces.
#[derive(Debug)]
pub struct EcdsaP256CertificateSigner<'a> {
    key: &'a EcdsaP256SigningKey,
}

impl<'a> EcdsaP256CertificateSigner<'a> {
    /// Wraps a signing key without taking ownership of it.
    #[must_use]
    pub const fn new(key: &'a EcdsaP256SigningKey) -> Self {
        Self { key }
    }

    /// Constructs the matching uncompressed P-256 subject-public-key encoding.
    ///
    /// # Errors
    ///
    /// Canonical DER construction fails.
    pub fn subject_public_key_info(&self) -> rsl_x509::Result<SubjectPublicKeyInfoDer> {
        SubjectPublicKeyInfoDer::ecdsa_p256(self.key.verifying_key().as_bytes())
    }
}

impl CertificateSigner for EcdsaP256CertificateSigner<'_> {
    type Error = IssuanceError;

    fn signature_algorithm(&self) -> rsl_x509::Result<SignatureAlgorithmDer> {
        SignatureAlgorithmDer::ecdsa_p256_sha256()
    }

    fn sign(&self, tbs_certificate: &[u8]) -> Result<Vec<u8>, Self::Error> {
        let signature = self.key.sign_sha256(tbs_certificate)?;
        Ok(ecdsa_signature_value(signature.as_bytes())?)
    }
}

/// ECDSA P-384/SHA-384 certificate signer with deterministic RFC 6979 nonces.
#[derive(Debug)]
pub struct EcdsaP384CertificateSigner<'a> {
    key: &'a EcdsaP384SigningKey,
}

impl<'a> EcdsaP384CertificateSigner<'a> {
    /// Wraps a signing key without taking ownership of it.
    #[must_use]
    pub const fn new(key: &'a EcdsaP384SigningKey) -> Self {
        Self { key }
    }

    /// Constructs the matching uncompressed P-384 subject-public-key encoding.
    ///
    /// # Errors
    ///
    /// Canonical DER construction fails.
    pub fn subject_public_key_info(&self) -> rsl_x509::Result<SubjectPublicKeyInfoDer> {
        SubjectPublicKeyInfoDer::ecdsa_p384(self.key.verifying_key().as_bytes())
    }
}

impl CertificateSigner for EcdsaP384CertificateSigner<'_> {
    type Error = IssuanceError;

    fn signature_algorithm(&self) -> rsl_x509::Result<SignatureAlgorithmDer> {
        SignatureAlgorithmDer::ecdsa_p384_sha384()
    }

    fn sign(&self, tbs_certificate: &[u8]) -> Result<Vec<u8>, Self::Error> {
        let signature = self.key.sign_sha384(tbs_certificate)?;
        Ok(ecdsa_signature_value(signature.as_bytes())?)
    }
}
