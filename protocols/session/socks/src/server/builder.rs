// --- Standard library ---
use std::marker::PhantomData;

// --- Internal modules ---
#[cfg(feature = "blocking")]
use super::blocking::Builder as BlockingBuilder;
#[cfg(feature = "mio")]
use super::mio::Builder as MioBuilder;
#[cfg(feature = "tokio")]
use super::tokio::Builder as TokioBuilder;
use super::{Limits, ServerConfig, Unselected, policy::Policy};
use crate::{Version, error::Error};

/// Builder for a server's accepted protocols, shared policy, and resource bounds.
/// Backend selection is required before construction:
/// ```compile_fail,E0599
/// use socks::Server;
///
/// let server = Server::configure().build();
/// ```
pub struct Builder<B = Unselected> {
    protocols: Vec<Version>,
    policy: Option<Policy>,
    limits: Limits,
    #[cfg(feature = "blocking")]
    pub(super) blocking_thread_budget: Option<usize>,
    backend: PhantomData<B>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            protocols: Vec::new(),
            policy: None,
            limits: Limits::default(),
            #[cfg(feature = "blocking")]
            blocking_thread_budget: None,
            backend: PhantomData,
        }
    }
}

impl Builder {
    /// Select blocking operations; selection is final and performs no I/O.
    ///
    /// ```compile_fail,E0599
    /// use socks::Server;
    ///
    /// Server::configure().blocking().blocking();
    /// ```
    #[cfg(feature = "blocking")]
    #[must_use]
    pub fn blocking(self) -> BlockingBuilder {
        self.select()
    }

    /// Select Mio readiness-driven proxy operations without creating a poll.
    ///
    /// ```compile_fail,E0599
    /// use socks::Server;
    ///
    /// Server::configure().mio().mio();
    /// ```
    #[cfg(feature = "mio")]
    #[must_use]
    pub fn mio(self) -> MioBuilder {
        self.select()
    }

    /// Select Tokio operations without binding a listener or creating a runtime.
    ///
    /// ```compile_fail,E0599
    /// use socks::Server;
    ///
    /// Server::configure().tokio().tokio();
    /// ```
    #[cfg(feature = "tokio")]
    #[must_use]
    pub fn tokio(self) -> TokioBuilder {
        self.select()
    }

    /// Move configuration into a backend-selected state.
    fn select<B>(self) -> Builder<B> {
        Builder {
            protocols: self.protocols,
            policy: self.policy,
            limits: self.limits,
            #[cfg(feature = "blocking")]
            blocking_thread_budget: self.blocking_thread_budget,
            backend: PhantomData,
        }
    }
}

impl<B> Builder<B> {
    /// Replace the accepted protocol set; duplicates are removed at construction.
    #[must_use]
    pub fn protocols(mut self, protocols: impl IntoIterator<Item = Version>) -> Self {
        self.protocols = protocols.into_iter().collect();
        self
    }

    /// Use this one policy for every accepted version and backend.
    #[must_use]
    pub fn policy(mut self, policy: Policy) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Set listener and phase limits.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Validate configuration without binding, resolving, or doing transport I/O.
    ///
    /// # Errors
    /// Returns missing-policy, empty-protocol-set, or invalid-limit errors.
    pub(super) fn into_config(self) -> Result<ServerConfig, Error> {
        let mut config =
            ServerConfig::new(self.protocols, self.policy.ok_or(Error::MissingPolicy)?);
        config.limits = self.limits;
        config.validate()?;
        Ok(config)
    }
}
