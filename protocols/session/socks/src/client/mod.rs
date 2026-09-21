//! Configured clients select a version-specific configuration, without fallback.
//!
//! ```
//! use socks::{Client, v5};
//! let client = Client::builder()
//!     .protocol(v5::client::Config::username_password(b"user", b"password")?)
//!     .build()?;
//! # Ok::<(), socks::error::Error>(())
//! ```

#[cfg(feature = "blocking")]
mod blocking;
mod builder;
mod client;
#[cfg(feature = "mio")]
mod mio;
mod protocol;
#[cfg(feature = "tokio")]
mod tokio;

pub use builder::Builder;
pub use client::Client;
pub use protocol::Protocol;
