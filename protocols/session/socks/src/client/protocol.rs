use crate::v5;

/// Configuration for one implemented protocol, including its authentication options.
#[non_exhaustive]
pub enum Protocol {
    /// SOCKS5 configuration.
    V5(v5::client::Config),
}

impl From<v5::client::Config> for Protocol {
    fn from(config: v5::client::Config) -> Self {
        Self::V5(config)
    }
}
