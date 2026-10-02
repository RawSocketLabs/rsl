// --- Workspace dependencies ---
use tokio::net::TcpListener;

// --- Internal modules ---
use super::super::ServerConfig;
use crate::proxy::tokio::Proxy;
use crate::{Version, error::Error};

/// Configured tokio server with explicit protocols and shared policy.
///
/// Construction performs no I/O; use the matching proxy driver for listener management.
/// Its asynchronous `exchange` authenticates an accepted TCP stream before authorization.
#[cfg_attr(
    feature = "blocking",
    doc = r"
A Tokio server cannot be passed to a blocking driver:
```compile_fail,E0308
use socks::{error::Error, server::tokio::Server};

fn wrong_driver(server: &Server, stream: std::net::TcpStream) -> Result<(), Error> {
    socks::proxy::blocking::serve_connection(stream, server)
}
```
"
)]
#[derive(Clone)]
pub struct Server {
    pub(crate) config: ServerConfig,
}

impl Server {
    /// Own a bound listener without starting service; call `Proxy::start` explicitly.
    pub fn into_proxy(self, listener: TcpListener) -> Proxy {
        Proxy::new(listener, self)
    }

    /// Validate an existing configuration for this backend.
    ///
    /// # Errors
    /// Rejects an empty protocol set or invalid resource limits.
    pub fn new(config: ServerConfig) -> Result<Self, Error> {
        config.validate()?;
        Ok(Self { config })
    }

    /// The explicitly accepted protocol versions.
    #[must_use]
    pub fn protocols(&self) -> &[Version] {
        self.config.protocols()
    }
}
