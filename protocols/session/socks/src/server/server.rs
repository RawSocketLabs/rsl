// --- Internal modules ---
use super::Builder;

/// Entry point for configuring a backend-specific server with one shared policy.
///
/// Select blocking, Tokio, or Mio before building. Construction validates settings
/// without binding a listener, authenticating a peer, or starting an event loop.
///
/// This entry point has no instances; built servers belong to their selected backend.
pub enum Server {}

impl Server {
    /// Configure a server; neither protocols nor policy has a default.
    #[must_use]
    pub fn configure() -> Builder {
        Builder::default()
    }
}
