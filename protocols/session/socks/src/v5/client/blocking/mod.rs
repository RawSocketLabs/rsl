//! Blocking SOCKS5 CONNECT and two-reply BIND exchanges.
mod bind;
mod connect;
mod exchange;
mod tcp_bind;

// --- Internal modules ---
pub use bind::{Binding, bind};
pub use connect::{connect, connect_with};
pub use tcp_bind::{TcpBinding, bind_tcp};
