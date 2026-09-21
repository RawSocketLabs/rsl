use super::{Builder, ServerConfig};
use crate::{Version, error::Error};

/// A configured server with explicit accepted protocols and one shared policy.
///
/// Construction performs no I/O. `exchange` (blocking) and `exchange_async`
/// (Tokio) authenticate and receive a request on an already connected TCP socket.
/// The returned exchange must be authorized before its target can be connected.
/// Neither method binds a listener or starts relaying application data.
///
/// Both features can coexist. With `mio`, pass this configuration to
/// `proxy::mio::Proxy` for the complete readiness-driven proxy; its embedded
/// `Exchange` is a separate manual policy/dial path. The default stays wire-only.
#[derive(Clone)]
pub struct Server {
    pub(crate) config: ServerConfig,
}

impl Server {
    /// Validate and retain explicit policies and resource limits.
    ///
    /// # Errors
    /// Returns [`Error::NoProtocols`] for an empty set, or [`Error::InvalidLimits`].
    pub fn new(config: ServerConfig) -> Result<Self, Error> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Start explicit construction; neither protocols nor policy has a default.
    #[must_use]
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// The versions accepted by this server.
    #[must_use]
    pub fn protocols(&self) -> &[Version] {
        self.config.protocols()
    }
}
