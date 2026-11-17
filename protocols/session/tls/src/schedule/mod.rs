//! The TLS 1.2 key schedule: the PRF, the master secret, and what is derived from it.

mod connection_end;
mod master_secret;
mod prf;

pub use connection_end::ConnectionEnd;
pub use master_secret::MasterSecret;
pub use prf::prf_sha256;
