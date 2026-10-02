//! Bounded Tokio acceptance and session cleanup for the owned proxy lifecycle.
// --- Standard library ---
use std::{
    future::Future, io, io::ErrorKind::Interrupted, net::SocketAddr, ops::ControlFlow, pin::Pin,
};

// --- Workspace dependencies ---
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
};

// --- Internal modules ---
use super::{Shutdown, event::Reporter};
use crate::{error::Error, server::tokio::Server};

/// Run a complete TCP proxy session with shared policy, phase deadlines, and
/// bidirectional half-close-aware relay. Async DNS is awaited within the connect
/// budget; cancellation cannot stop a system resolver call already running in
/// Tokio's blocking pool. Authentication/authorization callbacks must not block.
/// Accepts a validated Tokio server; construction and policy are shared across versions.
///
/// # Errors
/// Returns a terminal handshake, target, timeout, or relay error.
pub async fn serve_connection(stream: TcpStream, server: &Server) -> Result<(), Error> {
    let exchange = server.exchange(stream).await?;
    let connection = exchange.authorize().await?.connect().await?;
    connection.relay(server.config.limits.relay).await
}

/// Owned listening socket and bounded set of active session tasks.
pub(super) struct Listener {
    socket: TcpListener,
    workers: JoinSet<Result<(), Error>>,
}

impl Listener {
    /// Pair an already-bound listener with an empty session task set.
    pub(super) fn new(socket: TcpListener) -> Self {
        Self {
            socket,
            workers: JoinSet::new(),
        }
    }

    /// Run acceptance, then close the listener and join aborted sessions on every exit.
    pub(super) async fn serve(
        mut self,
        server: &Server,
        shutdown: watch::Receiver<bool>,
        reporter: Reporter,
    ) -> Result<(), Error> {
        let result = self.listen(server, shutdown, reporter).await;
        self.shutdown().await;
        result
    }

    /// Handle events until shutdown or a fatal failure.
    async fn listen(
        &mut self,
        server: &Server,
        shutdown: watch::Receiver<bool>,
        reporter: Reporter,
    ) -> Result<(), Error> {
        let shutdown = Shutdown::wait(shutdown);
        tokio::pin!(shutdown);

        while self
            .next(server, shutdown.as_mut(), &reporter)
            .await?
            .is_continue()
        {}

        Ok(())
    }

    /// Handle one event, prioritizing shutdown over completion and acceptance.
    async fn next(
        &mut self,
        server: &Server,
        shutdown: Pin<&mut impl Future<Output = ()>>,
        reporter: &Reporter,
    ) -> Result<ControlFlow<()>, Error> {
        let capacity = server.config.limits.connections;

        tokio::select! {
            biased;

            () = shutdown => return Ok(ControlFlow::Break(())),

            Some(comp) = self.workers.join_next(), if !self.workers.is_empty() => {
                comp.map_err(|_| Error::WorkerPanicked)?
                    .unwrap_or_else(|err| reporter.report(err));
            }

            accepted = self.socket.accept(), if self.workers.len() < capacity => {
                self.dispatch(accepted, server)?;
            }
        }

        Ok(ControlFlow::Continue(()))
    }

    /// Spawn an accepted socket; interruptions are retried by the scheduling loop.
    fn dispatch(
        &mut self,
        accepted: io::Result<(TcpStream, SocketAddr)>,
        server: &Server,
    ) -> Result<(), Error> {
        let (stream, _) = match accepted {
            Ok(connection) => connection,
            Err(err) if err.kind() == Interrupted => return Ok(()),
            Err(err) => return Err(err.into()),
        };

        let server = server.clone();
        self.workers
            .spawn(async move { serve_connection(stream, &server).await });

        Ok(())
    }

    /// Close acceptance before aborting every session and draining all task completions.
    async fn shutdown(mut self) {
        drop(self.socket);
        self.workers.abort_all();
        while self.workers.join_next().await.is_some() {}
    }
}
