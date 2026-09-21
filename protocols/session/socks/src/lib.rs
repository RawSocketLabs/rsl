//! SOCKS protocol wire codecs on [`bnb`].
//!
//! The current surface covers RFC 1928 SOCKS5 method negotiation, requests, replies, registries,
//! and typed IPv4/domain/IPv6 endpoints plus RFC 1929 username/password messages.
//! The optional `blocking`, `tokio`, and `mio` features add CONNECT clients, embeddable
//! server sessions, and bounded listening proxies. The default remains wire-only.
//!
//! # Organization
//!
//! - `client::Client` selects one typed version configuration; `server::Server`
//!   accepts an explicit set of versions under one `server::policy::Policy`.
//! - [`v5::wire`] provides raw codecs, also re-exported directly from [`v5`].
//! - `v5::client` and `v5::server` own version-specific blocking, Tokio, and Mio exchanges.
//! - `proxy` owns complete listeners and relay; `io` owns shared transport mechanics.
//! - `Destination` is an application address; `v5::Endpoint` also owns its wire `ATYP`.
//!
//! Only version 5 and CONNECT are implemented. The configured and version-specific
//! paths are distinct: embedded version APIs intentionally leave authorization and
//! dialing to their caller. Configured server stages enforce authorization before
//! dialing. No protocol, no-authentication policy, or destination allowlist is implicit.
//!
//! ```
//! use socks::v5::{Command, Endpoint, Request};
//! use std::net::Ipv4Addr;
//!
//! let request = Request::builder()
//!     .command(Command::Connect)
//!     .destination(Endpoint::Ipv4 {
//!         address: Ipv4Addr::new(127, 0, 0, 1),
//!         port: 443,
//!     })
//!     .build()
//!     .unwrap();
//! let wire = request.to_bytes().unwrap();
//! assert_eq!(Request::decode_exact(&wire).unwrap(), request);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod client;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod error;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod io;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod proxy;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub mod server;
mod types;
/// SOCKS version 5 wire types (RFC 1928 and RFC 1929).
pub mod v5;

#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub use client::Client;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub use io::stream::Stream;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub use server::{Connection, Server};
pub use types::{Destination, Version};
