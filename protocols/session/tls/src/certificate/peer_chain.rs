//! The certificates a peer presented, as received.

// --- Workspace dependencies ---
use x509::{Certificate, PublicKey};

// --- Internal modules ---
use super::CertificateError;

/// The DER certificates a peer sent in its `Certificate` message, leaf first.
///
/// The bytes are kept exactly as received and are not parsed on construction, so a caller
/// that does no verification (or pins the raw bytes) can still read a chain [`x509`] rejects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerChain {
    certificates: Vec<Vec<u8>>,
}

impl PeerChain {
    /// A chain of these DER certificates, leaf first (no validation).
    #[must_use]
    pub const fn from_der(certificates: Vec<Vec<u8>>) -> Self {
        Self { certificates }
    }

    /// Each certificate's DER bytes, leaf first, exactly as the peer sent them.
    #[must_use]
    pub fn der(&self) -> &[Vec<u8>] {
        &self.certificates
    }

    /// Every certificate parsed by [`x509`], leaf first.
    ///
    /// # Errors
    ///
    /// [`CertificateError::Parse`] naming the first certificate [`x509`] rejects.
    pub fn parse(&self) -> Result<Vec<Certificate<'_>>, CertificateError> {
        self.certificates
            .iter()
            .enumerate()
            .map(|(index, der)| parse_at(index, der))
            .collect()
    }

    /// The leaf's public key, which verifies the server's key-exchange signature.
    ///
    /// # Errors
    ///
    /// [`CertificateError::EmptyChain`] when there is no leaf, or [`CertificateError::Parse`]
    /// when the leaf or its key is not something [`x509`] accepts.
    pub fn leaf_public_key(&self) -> Result<PublicKey<'_>, CertificateError> {
        let der = self
            .certificates
            .first()
            .ok_or(CertificateError::EmptyChain)?;
        parse_at(0, der)?
            .tbs_certificate()
            .subject_public_key_info()
            .public_key()
            .map_err(|source| CertificateError::Parse { index: 0, source })
    }
}

/// Parses the certificate at `index`, attributing a failure to that position.
fn parse_at(index: usize, der: &[u8]) -> Result<Certificate<'_>, CertificateError> {
    Certificate::from_der(der).map_err(|source| CertificateError::Parse { index, source })
}
