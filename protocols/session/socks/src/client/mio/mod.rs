//! Configured Mio clients and caller-driven handshakes.
mod builder;
mod client;
mod handshake;

// --- Internal modules ---
pub use builder::Builder;
pub use client::Client;
pub use handshake::Handshake;
