//! Bounded Tokio listening proxy and half-close-aware relay.
use crate::io::tokio::bounded;
use crate::{Connection, Server, error::Error};
use std::{future::Future, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
impl Connection<TcpStream> {
    /// Relay both directions with half-close support and a total lifetime limit.
    /// Cancellation, I/O failure, or an expired deadline closes both owned sides.
    ///
    /// # Errors
    /// Returns invalid limits, I/O, or timeout errors.
    pub async fn relay(mut self, timeout: Duration) -> Result<(), Error> {
        bounded(timeout, async {
            tokio::io::copy_bidirectional(&mut self.client, &mut self.target).await?;
            Ok(())
        })
        .await
    }
}

/// Run a complete TCP proxy session with shared policy, phase deadlines, and
/// bidirectional half-close-aware relay. Async DNS is awaited within the connect
/// budget; cancellation cannot stop a system resolver call already running in
/// Tokio's blocking pool. Authentication/authorization callbacks must not block.
/// Accepts the validated server returned by [`Server::builder`] or [`Server::new`].
///
/// # Errors
/// Returns a terminal handshake, target, timeout, or relay error.
pub async fn serve_connection(stream: TcpStream, server: &Server) -> Result<(), Error> {
    let exchange = server.exchange_async(stream).await?;
    let connection = exchange.authorize().await?.connect().await?;
    connection.relay(server.config.limits.relay).await
}

/// Serve a TCP listener with bounded task concurrency until `shutdown` completes.
/// Stop acceptance, abort and join active session tasks on shutdown or listener
/// failure. Individual session failures go to `on_error`; they do not stop serving.
/// Saturation leaves connections in the OS backlog. Callbacks must not panic/block.
/// Sessions share the supplied server's validated configuration and policy.
///
/// # Errors
/// Returns a listener error or a worker panic.
pub async fn serve(
    listener: TcpListener,
    server: &Server,
    shutdown: impl Future<Output = ()>,
    on_error: impl Fn(Error),
) -> Result<(), Error> {
    let mut workers = JoinSet::new();
    tokio::pin!(shutdown);
    let result = loop {
        tokio::select! {
            biased;
            () = &mut shutdown => break Ok(()),
            result = workers.join_next(), if !workers.is_empty() => {
                match result {
                    Some(Ok(Err(error))) => on_error(error),
                    Some(Err(_)) => break Err(Error::WorkerPanicked),
                    _ => {}
                }
            }
            accepted = listener.accept(), if workers.len() < server.config.limits.connections => {
                match accepted {
                    Ok((stream, _)) => {
                        let server = server.clone();
                        workers.spawn(async move { serve_connection(stream, &server).await });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => break Err(error.into()),
                }
            }
        }
    };
    drop(listener);
    workers.abort_all();
    while workers.join_next().await.is_some() {}
    result
}
