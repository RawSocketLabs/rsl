// --- Internal modules ---
use crate::v5;

/// Settings for one implemented protocol version, including its authentication options.
#[non_exhaustive]
pub enum Configuration {
    /// SOCKS5 configuration.
    V5(v5::client::Config),
}

impl From<v5::client::Config> for Configuration {
    fn from(cfg: v5::client::Config) -> Self {
        Self::V5(cfg)
    }
}
