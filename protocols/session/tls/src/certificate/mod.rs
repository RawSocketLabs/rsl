//! The peer's certificate chain and the client's trust decision, built on [`x509`] and [`pki`].

mod error;
mod peer_chain;
mod server_verification;

pub use error::CertificateError;
pub use peer_chain::PeerChain;
pub use server_verification::ServerVerification;
