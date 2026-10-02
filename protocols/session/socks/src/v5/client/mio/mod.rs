//! Resumable SOCKS5 client and convenience poll loop.

mod client;
mod connect;

// --- Internal modules ---
pub use client::Client;
pub use connect::connect_tcp;
