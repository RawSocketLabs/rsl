//! Configured server and managed connection lifecycle.
//!
//! Select a backend before construction; policy and limits remain version-independent.
//! Building performs no I/O, and the returned server exposes only its backend's operations.
//! Blocking servers `serve` an owned listener directly. Tokio servers convert
//! `into_proxy`, then explicitly `start` the service.
//!
//! # Configuration and policy
//!
//! [`Server::configure`] starts a [`Builder`]; [`ServerConfig`] lets applications
//! reuse configuration across backend instances. [`Limits`] bounds resources and phases.
//! [`policy`] owns authentication requirements and destination authorization.
//!
//! # Example
//!
//! ```no_run
//! # #[cfg(feature = "tokio")]
//! # async fn example(stream: tokio::net::TcpStream) -> Result<(), socks::error::Error> {
//! use socks::{Server, Version, server::policy::{Policy, ServerAuth}};
//!
//! // Apply one explicit policy to every accepted version.
//! let server = Server::configure()
//!     .protocols([Version::V5])
//!     .policy(Policy::new(ServerAuth::no_authentication(), |context| {
//!         context.target.ip().is_loopback()
//!     }))
//!     .tokio()
//!     .build()?;
//!
//! // Authenticate, authorize, and connect before handing off application I/O.
//! let connection = server.exchange(stream).await?.authorize().await?.connect().await?;
//! let (client, target) = connection.into_parts();
//! # Ok(()) }
//! ```

//! # Backends
//!
//! Select one backend to obtain a concrete server; enabled backends can coexist.
//!
#![cfg_attr(
    feature = "blocking",
    doc = "- [`blocking`]: synchronous exchange, authorization, and connection stages."
)]
#![cfg_attr(
    feature = "tokio",
    doc = "- [`tokio`]: async exchange, authorization, and connection stages."
)]
//!
//! # Established connections
//!
//! [`Connection`] owns both relay sides after success. It differs from
//! [`crate::client::Connection`], which is the client's single application-facing stream.
//! Taking either connection apart preserves any unread buffered input.

mod backend;
#[cfg(feature = "blocking")]
pub mod blocking;
mod builder;
mod config;
mod connection;
pub mod policy;
pub(crate) mod resolve;
mod server;
#[cfg(feature = "tokio")]
pub mod tokio;

// --- Internal modules ---
#[cfg(feature = "blocking")]
pub use backend::Blocking;
#[cfg(feature = "tokio")]
pub use backend::Tokio;
pub use backend::Unselected;
pub use builder::Builder;
pub use config::{Limits, ServerConfig};
pub use connection::Connection;
pub use server::Server;
