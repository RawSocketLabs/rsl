use super::{Client, Protocol};
use crate::error::Error;
use std::time::{Duration, Instant};

/// Explicit client protocol selection and shared TCP setup budget.
pub struct Builder {
    protocol: Option<Protocol>,
    timeout: Duration,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            protocol: None,
            timeout: Duration::from_secs(10),
        }
    }
}

impl Builder {
    /// Select exactly one version through its typed configuration.
    #[must_use]
    pub fn protocol(mut self, protocol: impl Into<Protocol>) -> Self {
        self.protocol = Some(protocol.into());
        self
    }

    /// Set the absolute budget for TCP setup and the handshake, not application I/O.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Validate configuration without dialing or writing bytes.
    ///
    /// # Errors
    /// Requires a protocol and a nonzero, representable timeout.
    pub fn build(self) -> Result<Client, Error> {
        let protocol = self.protocol.ok_or(Error::MissingProtocol)?;
        if self.timeout.is_zero() || Instant::now().checked_add(self.timeout).is_none() {
            return Err(Error::InvalidLimits);
        }
        Ok(Client {
            protocol,
            timeout: self.timeout,
        })
    }
}
