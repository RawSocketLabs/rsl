// --- Standard library ---
use std::{net::TcpListener, sync::atomic::AtomicBool};

// --- Internal modules ---
use super::super::ServerConfig;
use crate::proxy::blocking::serve as serve_proxy;
use crate::{Version, error::Error};

/// Default per-listener budget for session workers and their relay threads.
pub(super) const DEFAULT_THREAD_BUDGET: usize = 128;

/// Configured blocking server with explicit protocols and shared policy.
///
/// Construction performs no I/O; use the matching proxy driver for listener management.
#[derive(Clone)]
pub struct Server {
    pub(crate) config: ServerConfig,
    thread_budget: usize,
}

impl Server {
    /// Serve an owned listener using this server's policy, limits, and thread budget.
    /// Shutdown closes acceptance and active sessions, then joins workers; blocking DNS
    /// and application callbacks can delay completion. Session errors go to `on_error`,
    /// which must not panic. This delegates to [`crate::proxy::blocking::serve`].
    ///
    /// # Errors
    /// Returns listener/thread creation errors or a worker panic.
    pub fn serve(
        &self,
        listener: TcpListener,
        shutdown: &AtomicBool,
        on_error: impl Fn(Error),
    ) -> Result<(), Error> {
        serve_proxy(listener, self, shutdown, on_error)
    }

    /// Validate an existing configuration with a 128-thread per-listener budget.
    ///
    /// # Errors
    /// Rejects an empty protocol set or invalid resource limits.
    pub fn new(config: ServerConfig) -> Result<Self, Error> {
        Self::with_thread_budget(config, DEFAULT_THREAD_BUDGET)
    }

    /// Validate both shared configuration and the listener's two-thread reservation budget.
    pub(super) fn with_thread_budget(
        config: ServerConfig,
        thread_budget: usize,
    ) -> Result<Self, Error> {
        config.validate()?;
        if thread_budget < 2 {
            return Err(Error::InvalidLimits);
        }
        Ok(Self {
            config,
            thread_budget,
        })
    }

    /// Maximum session/relay threads per `blocking::serve` listener (default: 128).
    /// Excludes the calling thread and threads created by application callbacks.
    /// Direct exchanges/`serve_connection` calls do not share this admission budget.
    #[must_use]
    pub fn thread_budget(&self) -> usize {
        self.thread_budget
    }

    /// Reserve a worker and relay thread for each admitted session, without multiplication.
    pub(crate) fn connection_capacity(&self) -> usize {
        self.config.limits.connections.min(self.thread_budget / 2)
    }

    /// The explicitly accepted protocol versions.
    #[must_use]
    pub fn protocols(&self) -> &[Version] {
        self.config.protocols()
    }
}

#[cfg(test)]
mod unit {
    // --- Internal modules ---
    use super::Server;
    use crate::{
        Version,
        error::Error,
        server::{
            ServerConfig,
            policy::{Policy, ServerAuth},
        },
    };

    fn config(connections: usize) -> ServerConfig {
        let mut cfg = ServerConfig::new(
            [Version::V5],
            Policy::new(ServerAuth::no_authentication(), |_| false),
        );
        cfg.limits.connections = connections;
        cfg
    }

    #[test]
    fn admission_reserves_two_threads_and_respects_both_bounds() {
        for (connections, budget, expected) in [
            (64, 2, 1),
            (64, 3, 1),
            (1, 128, 1),
            (64, 128, 64),
            (usize::MAX, usize::MAX, usize::MAX / 2),
        ] {
            let server = Server::with_thread_budget(config(connections), budget).unwrap();
            assert_eq!(
                server.connection_capacity(),
                expected,
                "connections={connections}, budget={budget}"
            );
        }
        let server = Server::new(config(usize::MAX)).unwrap();
        assert_eq!(server.thread_budget(), 128);
        assert_eq!(server.connection_capacity(), 64);
    }

    #[test]
    fn builder_rejects_budgets_that_cannot_reserve_one_session() {
        for budget in [0, 1, 2, 3, usize::MAX] {
            let server = crate::Server::configure()
                .protocols([Version::V5])
                .policy(Policy::new(ServerAuth::no_authentication(), |_| false))
                .blocking()
                .thread_budget(budget)
                .build();
            if budget < 2 {
                assert!(matches!(server, Err(Error::InvalidLimits)));
            } else {
                assert_eq!(server.unwrap().thread_budget(), budget);
            }
        }
    }
}
