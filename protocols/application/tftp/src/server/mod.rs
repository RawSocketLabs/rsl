//! Serving TFTP clients: the proxy-free file-transfer server of RFC 1350.
//!
//! [`Server`] binds a listen socket (port 69 by convention) and, for each
//! request, runs a complete transfer on its **own** freshly-bound socket — the
//! per-transfer transfer ID (TID) the protocol requires. What a download
//! returns and where an upload goes is decided by a [`Handler`]; the in-memory
//! [`MemoryStore`] is provided for tests and examples.
//!
//! Two ways to run it:
//!
//! - [`Server::serve_one`] handles exactly one transfer on the current thread
//!   (handy for tests and one-shot use).
//! - [`Server::serve`] loops, one thread per transfer, bounded by
//!   [`with_max_connections`](Server::with_max_connections).
//!
//! # Example
//!
//! ```no_run
//! use std::sync::Arc;
//! use tftp::server::{Handler, HandlerResult, Reject, Server};
//!
//! // A read-only handler that serves one in-memory file.
//! struct OneFile(Vec<u8>);
//! impl Handler for OneFile {
//!     fn read(&self, _name: &str) -> HandlerResult<Vec<u8>> { Ok(self.0.clone()) }
//!     fn write(&self, _n: &str, _c: Vec<u8>) -> HandlerResult<()> { Err(Reject::access_violation()) }
//! }
//!
//! # fn main() -> Result<(), tftp::error::TftpError> {
//! Server::bind("0.0.0.0:69", Arc::new(OneFile(b"hi".to_vec())))?.serve()?;
//! # Ok(())
//! # }
//! ```

mod handler;
mod server;

pub use handler::{Handler, HandlerResult, MemoryStore, Reject};
pub use server::{DEFAULT_MAX_CONNECTIONS, Server};
