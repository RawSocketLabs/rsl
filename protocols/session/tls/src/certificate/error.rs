//! Failures while parsing or validating a peer's certificate chain.

// --- Workspace dependencies ---
use thiserror::Error;

/// Why a peer's certificate chain was unusable.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CertificateError {
    /// The peer sent no certificates, so there is no leaf to use.
    #[error("the peer sent no certificates")]
    EmptyChain,
    /// [`x509`] rejected one certificate in the chain, or its public key.
    #[error("certificate {index} in the peer's chain was rejected by x509")]
    Parse {
        /// Position in the chain; the leaf is 0.
        index: usize,
        /// The parser's reason.
        #[source]
        source: x509::Error,
    },
    /// [`pki`] found no valid path from the leaf to a trust anchor.
    #[error("the peer's certificate chain failed path validation")]
    Path(#[source] pki::Error),
}
