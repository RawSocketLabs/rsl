//! SOCKS5 client implementations, grouped by I/O backend.

#[cfg(feature = "blocking")]
pub mod blocking;
mod config;
#[cfg(feature = "mio")]
pub mod mio;
#[cfg(feature = "tokio")]
pub mod tokio;

pub use config::Config;
