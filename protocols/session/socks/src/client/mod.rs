//! Driving a SOCKS proxy: the client side.
//!
//! This module hosts a client per protocol version. The names at the top level
//! (`Client`, `TargetAddr`, …) are the **SOCKS5** client (`v5` feature); the
//! **SOCKS4 / 4A** client lives in the [`v4`] submodule (`v4` feature). Each
//! version exposes exactly what it can express, so there are no methods that
//! error at runtime for an unsupported capability.
//!
//! # SOCKS5
//!
//! [`Client`] is the entry point. It negotiates an authentication method, issues
//! one of the three SOCKS5 commands, and hands back a transport you use as if it
//! reached the target directly:
//!
//! | Command | Method | Returns |
//! |---|---|---|
//! | CONNECT | [`Client::connect`] | a transparent [`TcpStream`](std::net::TcpStream) |
//! | BIND | [`Client::bind`] | a [`BindListener`] that yields the inbound peer |
//! | UDP ASSOCIATE | [`Client::udp_associate`] | a [`UdpTunnel`] for datagrams |
//!
//! A connection target is a [`TargetAddr`] — either a socket address or a
//! domain name the *proxy* resolves (so DNS need not leak from the client).
//! Most methods accept anything that converts into one, including
//! `("host", port)` tuples and [`SocketAddr`](std::net::SocketAddr).
//!
//! For deliberately malformed traffic — fuzzing, interop probing, security
//! research — drop to the [`raw`] module, which performs no negotiation and
//! validates nothing.
//!
//! # Example
//!
//! ```no_run
//! # #[cfg(feature = "v5")] {
//! use std::io::{Read, Write};
//! use socks::client::Client;
//!
//! # fn main() -> Result<(), socks::error::SocksError> {
//! let proxy = "127.0.0.1:1080".parse()?;
//! let mut stream = Client::with_user_pass(proxy, "admin", "hunter2")?
//!     .connect(("example.com", 443))?;
//! stream.write_all(b"hello")?;
//! let mut reply = [0u8; 5];
//! stream.read_exact(&mut reply)?;
//! # Ok(())
//! # }
//! # }
//! ```

#[cfg(feature = "v5")]
mod bind;
#[cfg(feature = "v5")]
mod client;
#[cfg(feature = "v5")]
pub mod raw;
#[cfg(feature = "v5")]
mod udp;

#[cfg(feature = "v4")]
pub mod v4;

#[cfg(feature = "v5")]
pub use bind::BindListener;
#[cfg(feature = "v5")]
pub use client::{Client, TargetAddr};
#[cfg(feature = "v5")]
pub use raw::RawClient;
#[cfg(feature = "v5")]
pub use udp::UdpTunnel;
