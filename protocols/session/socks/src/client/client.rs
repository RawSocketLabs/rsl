// --- Standard library ---
use std::time::Duration;

// --- Internal modules ---
use super::{Builder, Configuration};
use crate::Version;

/// Entry point for configuring a backend-specific client.
/// Select a backend on the builder to obtain a client with connection methods.
/// Shared settings are retained internally; no backend-free client can be built publicly.
pub struct Client {
    pub(super) configuration: Configuration,
    #[cfg_attr(
        not(any(feature = "blocking", feature = "tokio")),
        expect(
            dead_code,
            reason = "Stored configuration is consumed only by enabled TCP drivers"
        )
    )]
    pub(super) timeout: Duration,
}

impl Client {
    /// Configure a client with required version-specific settings and a ten-second TCP budget.
    ///
    /// A configuration cannot be omitted or silently defaulted:
    /// ```compile_fail
    /// let client = socks::Client::configure().build();
    /// ```
    #[must_use]
    pub fn configure(configuration: impl Into<Configuration>) -> Builder {
        Builder::new(configuration.into())
    }

    /// The selected version; never changes in response to a failed connection.
    #[must_use]
    pub const fn version(&self) -> Version {
        match self.configuration {
            Configuration::V5(_) => Version::V5,
        }
    }
}
