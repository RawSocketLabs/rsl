// --- Internal modules ---
use super::super::{Blocking, Builder as SettingsBuilder};
use super::Server;
use super::server::DEFAULT_THREAD_BUDGET;
use crate::error::Error;

/// Backend-selected server builder; shared options remain available.
pub type Builder = SettingsBuilder<Blocking>;

impl SettingsBuilder<Blocking> {
    /// Bound session and relay threads per blocking listener (default: 128).
    /// Each accepted session reserves two slots, including during its handshake.
    /// The effective connection capacity is `min(limits.connections, budget / 2)`;
    /// odd budgets leave one slot unused. Budgets below two are rejected by `build`.
    /// This excludes the calling thread and callback-created threads, and does not
    /// limit direct exchanges or `serve_connection` calls. It is not process-wide.
    ///
    /// ```
    /// use socks::{Server, Version, server::policy::{Policy, ServerAuth}};
    ///
    /// let server = Server::configure()
    ///     .protocols([Version::V5])
    ///     .policy(Policy::new(ServerAuth::no_authentication(), |_| false))
    ///     .blocking()
    ///     .thread_budget(32)
    ///     .build()?;
    /// assert_eq!(server.thread_budget(), 32);
    /// # Ok::<(), socks::error::Error>(())
    /// ```
    #[must_use]
    pub fn thread_budget(mut self, budget: usize) -> Self {
        self.blocking_thread_budget = Some(budget);
        self
    }

    /// Validate configuration without binding a listener or performing I/O.
    ///
    /// # Errors
    /// Rejects missing policy, empty protocols, or invalid resource limits.
    pub fn build(self) -> Result<Server, Error> {
        let budget = self.blocking_thread_budget.unwrap_or(DEFAULT_THREAD_BUDGET);
        Server::with_thread_budget(self.into_config()?, budget)
    }
}
