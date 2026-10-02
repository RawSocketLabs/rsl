//! Established client connection and backend-specific I/O delegation.
mod connection;
#[cfg(any(feature = "blocking", feature = "mio"))]
mod std;
#[cfg(feature = "tokio")]
mod tokio;

// --- Internal modules ---
pub use connection::Connection;
