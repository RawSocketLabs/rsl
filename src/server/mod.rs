//! Serving SOCKS5 clients: the proxy side of RFC 1928.
//!
//! [`Server`] binds a listener and, for each client, runs the full session:
//! method negotiation (RFC 1928 §3), authentication subnegotiation
//! ([`auth`](crate::auth)), the command request (§4), and then relays bytes (or
//! datagrams) until either side closes. All three commands are handled —
//! CONNECT, BIND, and UDP ASSOCIATE.
//!
//! Two ways to run it:
//!
//! - [`Server::accept`] serves exactly one client on the current thread (handy
//!   for tests and one-shot use).
//! - [`Server::serve`] loops forever, one thread per connection, bounded by
//!   [`with_max_connections`](Server::with_max_connections).
//!
//! The server defends against resource-exhaustion by default: a
//! [handshake timeout](Server::with_handshake_timeout) bounds stalled
//! negotiations ([`DEFAULT_HANDSHAKE_TIMEOUT`]), a
//! [bind timeout](Server::with_bind_timeout) bounds a BIND waiting for a peer
//! that never arrives ([`DEFAULT_BIND_TIMEOUT`]), and concurrency is capped
//! ([`DEFAULT_MAX_CONNECTIONS`]). UDP relays additionally pin replies to the
//! association's client address (RFC 1928 §7) to resist injection.
//!
//! # Example
//!
//! ```no_run
//! use socks::server::Server;
//!
//! # fn main() -> Result<(), socks::error::SocksError> {
//! // Unauthenticated proxy; serve clients until the listener fails.
//! Server::bind("127.0.0.1:1080")?.serve()?;
//! # Ok(())
//! # }
//! ```

mod connection;
mod relay;
mod server;
mod udp;

pub use server::{
    Server, DEFAULT_BIND_TIMEOUT, DEFAULT_HANDSHAKE_TIMEOUT, DEFAULT_MAX_CONNECTIONS,
};
