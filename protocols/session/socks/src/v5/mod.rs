//! SOCKS5 wire codecs and version-specific client/server exchanges.
//!
//! # Messages and authentication
//!
//! [`wire`](crate::v5::wire) owns the SOCKS5 message types, also re-exported here for shorter paths.
//! [`auth`](crate::v5::auth) contains the client authentication choices used by version-specific exchanges;
//! credential wire messages remain in `wire`.
//!
//! # Protocol roles
//!
//! [`client`](crate::v5::client) groups version-specific configuration and client backends.
#![cfg_attr(
    any(feature = "blocking", feature = "tokio", feature = "mio"),
    doc = "[`server`](crate::v5::server) groups embedded server exchanges by backend. These leave policy/dialing to their caller; use [`crate::server`] for configured authorization stages."
)]

pub mod auth;
pub mod client;
#[cfg(feature = "mio")]
pub(crate) mod mio_io;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod server;
pub mod wire;

// --- Internal modules ---
pub use wire::*;
