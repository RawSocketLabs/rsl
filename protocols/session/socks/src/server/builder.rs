use super::{Limits, Server, ServerConfig, policy::Policy};
use crate::{Version, error::Error};

/// Builder for a server's accepted protocols, shared policy, and resource bounds.
#[derive(Default)]
pub struct Builder {
    protocols: Vec<Version>,
    policy: Option<Policy>,
    limits: Limits,
}

impl Builder {
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
    pub fn build(self) -> Result<Server, Error> {
        let mut config =
            ServerConfig::new(self.protocols, self.policy.ok_or(Error::MissingPolicy)?);
        config.limits = self.limits;
        Server::new(config)
    }
}
