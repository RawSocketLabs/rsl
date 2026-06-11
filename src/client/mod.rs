//! Driving a SOCKS5 proxy: the client side of RFC 1928.
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
//! ```

mod bind;
mod client;
pub mod raw;
mod udp;

pub use bind::BindListener;
pub use client::{Client, TargetAddr};
pub use raw::RawClient;
pub use udp::UdpTunnel;
