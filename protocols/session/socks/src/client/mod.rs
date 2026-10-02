//! Configured clients select a version-specific configuration, without fallback.
//! Configuration is available with default features. Select `blocking`
//! or `tokio` on the builder to produce the corresponding concrete client.
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
//! # #[cfg(all(feature = "blocking", feature = "tokio"))]
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
//! let _connection = asynchronous.connect(proxy, dest).await?;
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
    feature = "tokio",
    doc = "- [`tokio`]: asynchronous connections and caller-supplied Tokio I/O."
)]
#![cfg_attr(
    not(any(feature = "blocking", feature = "tokio")),
    doc = "No backend is enabled in this build. Enable `blocking` or `tokio` to construct a client."
)]
#![cfg_attr(
    any(feature = "blocking", feature = "tokio"),
    doc = "\n# Established connections\n\n[`Connection`] owns the negotiated stream and bound address. Read/write through it, or use its lossless `into_parts()` handoff."
)]

mod backend;
#[cfg(feature = "blocking")]
pub mod blocking;
mod builder;
mod client;
mod configuration;
#[cfg(any(feature = "blocking", feature = "tokio"))]
mod connection;
#[cfg(feature = "tokio")]
pub mod tokio;

// --- Internal modules ---
#[cfg(feature = "blocking")]
pub use backend::Blocking;
#[cfg(feature = "tokio")]
pub use backend::Tokio;
pub use backend::Unselected;
pub use builder::Builder;
pub use client::Client;
pub use configuration::Configuration;
#[cfg(any(feature = "blocking", feature = "tokio"))]
pub use connection::Connection;
