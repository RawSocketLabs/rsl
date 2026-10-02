//! Embedded blocking exchanges; callers own request authorization and relay.
mod connect;
mod exchange;

// --- Internal modules ---
pub use connect::{Request, exchange};
