//! Blocking SOCKS5 CONNECT exchange.
mod connect;
mod exchange;

// --- Internal modules ---
pub use connect::{connect, connect_with};
