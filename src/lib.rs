/*!
A SOCKS Protocol Version 5 library with three API layers:
1. High-level client and server workflows for CONNECT, BIND, and UDP ASSOCIATE.
2. Mid-level authentication negotiation via pluggable handlers.
3. Low-level wire-format primitives for SOCKS5 messages.

Primary protocol references:
- RFC 1928 (SOCKS Protocol Version 5)
- RFC 1929 (Username/Password Authentication for SOCKS V5)

GSS-API authentication (RFC 1961) is not implemented here; the
[`auth::Authenticator`] and [`auth::AuthHandler`] traits are the extension
point for a future helper crate to provide it.

# Logging

The crate is instrumented with the [`tracing`] facade (connection lifecycle,
method selection, command dispatch, relay byte counts, auth outcomes). Install
a subscriber to see output; with none installed the cost is a cached atomic
check per call site. To compile the instrumentation out entirely, depend on
tracing with `features = ["release_max_level_off"]`.

[`tracing`]: https://docs.rs/tracing
*/

/// Authentication method negotiation traits and built-in implementations.
pub mod auth;

/// High level API for connecting through a SOCKS5 proxy.
pub mod client;

/// Shared error type for the library.
pub mod error;

/// High level API for serving SOCKS5 clients.
pub mod server;

/// Wire-format primitives for SOCKS protocol version 5.
pub mod v5;
