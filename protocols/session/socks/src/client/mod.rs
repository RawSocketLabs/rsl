//! Configured clients select a version-specific configuration, without fallback.
//! Configuration is available with default features. Select `blocking`,
//! `tokio`, or `mio` on the builder to produce the corresponding concrete client.
//! Constructing any client performs no I/O.
//!
//! # Configuration
//!
//! [`Client::configure`] starts a [`Builder`] with a required [`Configuration`].
//! Shared options work before or after backend selection; only a selected builder can build.
//!
//! # Examples
//!
//! ```
//! use socks::{Client, v5};
//!
//! // Configure credentials without opening a connection.
//! let cfg = v5::client::Config::username_password(b"user", b"password")?;
//! let configuration = Client::configure(cfg);
//! # Ok::<(), socks::error::Error>(())
//! ```
//!
//! ```no_run
//! # #[cfg(all(feature = "blocking", feature = "tokio", feature = "mio"))]
//! # async fn example() -> Result<(), socks::error::Error> {
//! use socks::{Client, Destination, v5::client::Config};
//!
//! let proxy = "127.0.0.1:1080".parse().unwrap();
//! let dest = Destination::domain(b"example.com", 443);
//!
//! // Blocking setup returns an established connection.
//! let blocking = Client::configure(Config::no_authentication()).blocking().build()?;
//! let _connection = blocking.connect(proxy, dest.clone())?;
//!
//! // Tokio exposes the same operation asynchronously.
//! let asynchronous = Client::configure(Config::no_authentication()).tokio().build()?;
//! let _connection = asynchronous.connect(proxy, dest.clone()).await?;
//!
//! let readiness = Client::configure(Config::no_authentication()).mio().build()?;
//! // Native event loops use readiness.connect_with(connected_nonblocking_stream, dest).
//! // This separately named convenience instead drives its own poll loop:
//! let (_poll, _connection) = readiness.connect_blocking(proxy, dest)?;
//! # Ok(()) }
//! ```

//! # Backends
//!
//! Each enabled module exposes a concrete client and an alias for its selected builder.
//!
#![cfg_attr(
    feature = "blocking",
    doc = "- [`blocking`]: synchronous connections and caller-supplied standard I/O."
)]
#![cfg_attr(
    feature = "mio",
    doc = "- [`mio`]: resumable handshakes for event loops; a separately named convenience drives its own poll."
)]
#![cfg_attr(
    feature = "tokio",
    doc = "- [`tokio`]: asynchronous connections and caller-supplied Tokio I/O."
)]
#![cfg_attr(
    not(any(feature = "blocking", feature = "tokio", feature = "mio")),
    doc = "No backend is enabled in this build. Enable `blocking`, `tokio`, or `mio` to construct a client."
)]
#![cfg_attr(
    any(feature = "blocking", feature = "tokio", feature = "mio"),
    doc = "\n# Established connections\n\n[`Connection`] owns the negotiated stream and bound address. Read/write through it, or use its lossless `into_parts()` handoff."
)]

mod backend;
#[cfg(feature = "blocking")]
pub mod blocking;
mod builder;
mod client;
mod configuration;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
mod connection;
#[cfg(feature = "mio")]
pub mod mio;
#[cfg(feature = "tokio")]
pub mod tokio;

// --- Internal modules ---
#[cfg(feature = "blocking")]
pub use backend::Blocking;
#[cfg(feature = "mio")]
pub use backend::Mio;
#[cfg(feature = "tokio")]
pub use backend::Tokio;
pub use backend::Unselected;
pub use builder::Builder;
pub use client::Client;
pub use configuration::Configuration;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub use connection::Connection;
