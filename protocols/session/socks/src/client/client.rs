use super::{Builder, Protocol};
use crate::Version;
use std::time::Duration;

/// Reusable client configuration. Construction performs no I/O.
/// TCP convenience methods share one absolute connect/handshake budget; generic
/// transport handshakes retain caller-managed deadlines. No implicit downgrade.
pub struct Client {
    pub(super) protocol: Protocol,
    pub(super) timeout: Duration,
}

impl Client {
    /// Begin configuration with an explicit protocol and default ten-second TCP budget.
    #[must_use]
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// The selected version; never changes in response to a failed connection.
    #[must_use]
    pub const fn version(&self) -> Version {
        match self.protocol {
            Protocol::V5(_) => Version::V5,
        }
    }
}
