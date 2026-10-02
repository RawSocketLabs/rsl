//! SOCKS5 client implementations, grouped by I/O backend.
//!
//! # Configuration
//!
//! [`Config`] selects authentication for the version-independent [`crate::Client`].
//! Direct backend functions below provide the embedded, SOCKS5-specific path.
//!
//! # Backends
//!
#![cfg_attr(
    feature = "blocking",
    doc = "- [`blocking`]: CONNECT over TCP or a supplied transport."
)]
#![cfg_attr(
    feature = "tokio",
    doc = "- [`tokio`]: async CONNECT over TCP or a supplied transport."
)]
#![cfg_attr(
    not(any(feature = "blocking", feature = "tokio")),
    doc = "No transport backend is enabled; [`Config`] remains available."
)]

#[cfg(feature = "blocking")]
pub mod blocking;
mod config;
#[cfg(feature = "tokio")]
pub mod tokio;

// --- Internal modules ---
pub use config::Config;
