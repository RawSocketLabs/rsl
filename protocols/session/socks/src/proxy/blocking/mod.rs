//! Bounded blocking listening proxy and half-close-aware relay.

mod listener;
mod sockets;
mod worker;

// --- Internal modules ---
pub use listener::{serve, serve_connection};
