//! SOCKS protocol wire codecs on [`bnb`].
//!
//! The current surface covers RFC 1928 SOCKS5 method negotiation, requests, replies, registries,
//! and typed IPv4/domain/IPv6 endpoints plus RFC 1929 username/password messages.
//! The optional `blocking` and `tokio` features add CONNECT clients, embeddable
//! server sessions, and bounded listening proxies. The default remains wire-only.
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

/// SOCKS version 5 wire types (RFC 1928 and RFC 1929).
pub mod v5;

#[cfg(feature = "tokio")]
pub mod asynchronous;
#[cfg(feature = "blocking")]
pub mod blocking;
#[cfg(any(feature = "blocking", feature = "tokio"))]
pub mod error;
#[cfg(any(feature = "blocking", feature = "tokio"))]
pub mod session;
