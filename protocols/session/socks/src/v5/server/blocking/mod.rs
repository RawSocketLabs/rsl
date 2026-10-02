//! Embedded blocking exchanges; callers own request authorization and relay.
mod bind;
mod bind_listener;
mod connect;
mod exchange;
mod tcp_bind;

// --- Internal modules ---
pub use bind::{AwaitingPeer, BindRequest, exchange_bind};
pub use connect::{Request, exchange};
pub use tcp_bind::{TcpAwaitingPeer, TcpBindRequest, exchange_bind_tcp};
