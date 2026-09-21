//! Accepted protocols, policy, and resource bounds for servers and proxies.

use super::policy::Policy;
use crate::{Version, error::Error};
use std::time::Duration;

/// Accepted protocols, one shared security policy, and transport resource bounds.
/// Version-specific wire/authentication behavior lives under each version module.
#[derive(Clone)]
pub struct ServerConfig {
    protocols: Vec<Version>,
    /// One policy applied to every accepted protocol and I/O backend.
    pub policy: Policy,
    /// Resource bounds validated before accepting work.
    pub limits: Limits,
}

impl ServerConfig {
    /// Configure an explicit protocol set and policy. Duplicates are ignored.
    /// Validation happens in [`Server::new`](super::Server::new) and complete proxy entry points.
    #[must_use]
    pub fn new(protocols: impl IntoIterator<Item = Version>, policy: Policy) -> Self {
        let mut accepted = Vec::new();
        for protocol in protocols {
            if !accepted.contains(&protocol) {
                accepted.push(protocol);
            }
        }
        Self {
            protocols: accepted,
            policy,
            limits: Limits::default(),
        }
    }

    /// The explicitly accepted versions, with duplicates removed.
    #[must_use]
    pub fn protocols(&self) -> &[Version] {
        &self.protocols
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.protocols.is_empty() {
            return Err(Error::NoProtocols);
        }
        self.limits.validate()
    }

    // With one implementation no transport sniff is needed. Future multi-version
    // dispatch must inspect retained input without consuming its version byte.
    pub(crate) fn protocol(&self) -> Version {
        self.protocols[0]
    }
}

/// Bounds used by the listening proxies and their TCP per-connection handlers.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Absolute deadline for negotiation, authentication, and receiving CONNECT.
    pub handshake: Duration,
    /// Budget shared by outbound connection attempts (not blocking system DNS).
    pub connect: Duration,
    /// Total relay lifetime in every driver, not an idle timeout.
    pub relay: Duration,
    /// Maximum simultaneous sessions accepted by a listening proxy.
    pub connections: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            handshake: Duration::from_secs(10),
            connect: Duration::from_secs(10),
            relay: Duration::from_secs(300),
            connections: 64,
        }
    }
}

impl Limits {
    pub(crate) fn validate(self) -> Result<(), Error> {
        if self.handshake.is_zero()
            || self.connect.is_zero()
            || self.relay.is_zero()
            || self.connections == 0
            || std::time::Instant::now()
                .checked_add(self.handshake)
                .is_none()
            || std::time::Instant::now()
                .checked_add(self.connect)
                .is_none()
            || std::time::Instant::now().checked_add(self.relay).is_none()
        {
            Err(Error::InvalidLimits)
        } else {
            Ok(())
        }
    }
}
