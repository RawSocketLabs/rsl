// --- Standard library ---
use std::marker::PhantomData;
use std::time::{Duration, Instant};

// --- Internal modules ---
use super::Unselected;
#[cfg(feature = "blocking")]
use super::blocking::Builder as BlockingBuilder;
#[cfg(feature = "mio")]
use super::mio::Builder as MioBuilder;
#[cfg(feature = "tokio")]
use super::tokio::Builder as TokioBuilder;
use super::{Client, Configuration};
use crate::{error::Error, io::deadline::deadline};

/// Required version-specific configuration and optional shared TCP setup budget.
/// Select a backend before calling its builder's `build` method.
/// `B` records that choice at compile time; it stores no runtime backend value.
/// Shared options remain available before and after selection.
///
/// ```
/// use socks::client::Builder;
/// use std::time::Duration;
///
/// // Helpers can configure any backend without changing its selected state.
/// fn with_budget<B>(builder: Builder<B>) -> Builder<B> {
///     builder.timeout(Duration::from_secs(5))
/// }
/// ```
///
/// ```compile_fail,E0599
/// use socks::{Client, v5::client::Config};
///
/// let client = Client::configure(Config::no_authentication()).build();
/// ```
pub struct Builder<B = Unselected> {
    configuration: Configuration,
    timeout: Duration,
    backend: PhantomData<B>,
}

impl Builder {
    /// Select the blocking client backend without performing I/O.
    ///
    /// Backend selection is final; selected builders expose settings and construction only.
    /// ```compile_fail,E0599
    /// use socks::{Client, v5::client::Config};
    ///
    /// Client::configure(Config::no_authentication()).blocking().blocking();
    /// ```
    #[cfg(feature = "blocking")]
    #[must_use]
    pub fn blocking(self) -> BlockingBuilder {
        self.select()
    }

    /// Select the Tokio client backend without performing I/O.
    ///
    /// ```compile_fail,E0599
    /// use socks::{Client, v5::client::Config};
    ///
    /// Client::configure(Config::no_authentication()).tokio().tokio();
    /// ```
    #[cfg(feature = "tokio")]
    #[must_use]
    pub fn tokio(self) -> TokioBuilder {
        self.select()
    }

    /// Select the Mio client backend for caller-driven readiness.
    ///
    /// ```compile_fail,E0599
    /// use socks::{Client, v5::client::Config};
    ///
    /// Client::configure(Config::no_authentication()).mio().mio();
    /// ```
    #[cfg(feature = "mio")]
    #[must_use]
    pub fn mio(self) -> MioBuilder {
        self.select()
    }

    /// Start with a required configuration and the default ten-second TCP budget.
    pub(super) fn new(configuration: Configuration) -> Self {
        Self {
            configuration,
            timeout: Duration::from_secs(10),
            backend: PhantomData,
        }
    }

    /// Move settings into the selected backend state without copying credentials.
    #[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
    fn select<B>(self) -> Builder<B> {
        Builder {
            configuration: self.configuration,
            timeout: self.timeout,
            backend: PhantomData,
        }
    }
}

impl<B> Builder<B> {
    /// Set the absolute budget for TCP setup and the handshake, not application I/O.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Validate shared settings for a selected backend without performing I/O.
    ///
    /// # Errors
    /// Requires a nonzero, representable timeout.
    #[cfg_attr(
        not(any(test, feature = "blocking", feature = "tokio", feature = "mio")),
        expect(
            dead_code,
            reason = "Only enabled backend builders consume shared settings"
        )
    )]
    pub(super) fn into_settings(self) -> Result<Client, Error> {
        deadline(Instant::now(), self.timeout)?;
        Ok(Client {
            configuration: self.configuration,
            timeout: self.timeout,
        })
    }
}

#[cfg(test)]
mod unit {
    // --- Internal modules ---
    use super::*;
    use crate::{Version, v5::client::Config};

    #[test]
    fn shared_settings_validate_timeout_before_backend_construction() {
        for timeout in [Duration::ZERO, Duration::MAX] {
            assert!(matches!(
                Client::configure(Config::no_authentication())
                    .timeout(timeout)
                    .into_settings(),
                Err(Error::InvalidLimits)
            ));
        }
        let settings = Client::configure(Config::no_authentication())
            .into_settings()
            .unwrap();
        assert_eq!(settings.version(), Version::V5);
    }
}
