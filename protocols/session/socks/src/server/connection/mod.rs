//! Established server connections and backend-specific relay behavior.
#[cfg(feature = "blocking")]
mod blocking;
mod connection;
#[cfg(feature = "tokio")]
mod tokio;

// --- Internal modules ---
pub use connection::Connection;
