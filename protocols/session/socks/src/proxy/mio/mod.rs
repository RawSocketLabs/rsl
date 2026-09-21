//! Owned-poll proxy with bounded sessions, DNS workers, and relay storage.
//!
//! ```no_run
//! use socks::{Server, Version, server::policy::{Policy, ServerAuth}, proxy::mio::Proxy};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let allowed: std::net::SocketAddr = "127.0.0.1:8080".parse()?;
//! let server = Server::builder().protocols([Version::V5])
//!     .policy(Policy::new(ServerAuth::no_authentication(), move |context| context.target == allowed))
//!     .build()?;
//! let listener = mio::net::TcpListener::bind("127.0.0.1:1080".parse()?)?;
//! let mut proxy = Proxy::new(listener, server)?;
//! let shutdown = proxy.shutdown_handle(); // Give to the application controller.
//! proxy.run(|_error| { /* report without credentials */ })?;
//! # Ok(()) }
//! ```

mod entry;
mod proxy;
mod relay;
mod resolver;
mod shutdown;

pub use proxy::Proxy;
pub use shutdown::Shutdown;
