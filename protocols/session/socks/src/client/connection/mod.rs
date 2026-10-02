//! Established client connection and backend-specific I/O delegation.
mod connection;
#[cfg(feature = "blocking")]
mod std;
#[cfg(feature = "tokio")]
mod tokio;

// --- Internal modules ---
pub use connection::Connection;
