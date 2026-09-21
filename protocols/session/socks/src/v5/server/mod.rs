//! SOCKS5 server implementations, grouped by I/O backend.
#[cfg(feature = "blocking")]
pub mod blocking;
#[cfg(feature = "mio")]
pub mod mio;
#[cfg(feature = "tokio")]
pub mod tokio;
mod validation;
