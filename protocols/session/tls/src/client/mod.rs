//! The sans-I/O TLS 1.2 client: protocol state without sockets, runtimes or clocks.
//!
//! A driver moves bytes between its transport and the [`Client`]: it sends what
//! [`Client::transmit`] holds and feeds what it reads to [`Client::receive`]. The client owns
//! sequencing, validation and every secret.

mod client;
mod config;
mod error;

pub use client::Client;
pub use config::ClientConfig;
pub use error::Error;
