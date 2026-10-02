//! Owned Tokio proxy lifecycle and a caller-managed per-connection driver.

mod event;
mod listener;
mod proxy;
mod running;
mod shutdown;

// --- Internal modules ---
pub use listener::serve_connection;
pub use proxy::Proxy;
pub use running::Running;
pub use shutdown::Shutdown;
