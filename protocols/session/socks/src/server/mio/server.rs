// --- Workspace dependencies ---
use mio::net::TcpListener;

// --- Internal modules ---
use super::super::ServerConfig;
use crate::proxy::mio::Proxy;
use crate::{Version, error::Error};

/// Configured mio server with explicit protocols and shared policy.
///
/// Construction performs no I/O; use the matching proxy driver for listener management.
/// Use [`Self::into_proxy`] to create the readiness-driven
/// listener. Building a server does not create a poll or launch an event loop.
#[derive(Clone)]
pub struct Server {
    pub(crate) config: ServerConfig,
}

impl Server {
    /// Consume this configuration and an owned listener to create a driveable proxy.
    /// Creates its poll and resolution workers, but does not start the event loop.
    /// Use the returned proxy's `run` or `poll` methods and `shutdown_handle`.
    /// Equivalent to [`Proxy::new`]; unlike blocking/Tokio `serve`, this returns control
    /// to the caller before accepting sessions.
    ///
    /// # Errors
    /// Returns invalid limits, listener registration, poll creation, allocation,
    /// or resolver-worker setup errors.
    pub fn into_proxy(self, listener: TcpListener) -> Result<Proxy, Error> {
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
