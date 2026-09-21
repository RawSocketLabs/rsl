//! SOCKS5 wire codecs and version-specific client/server exchanges.

#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod auth;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod client;
#[cfg(feature = "mio")]
pub(crate) mod mio_io;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod server;
pub mod wire;

pub use wire::*;
