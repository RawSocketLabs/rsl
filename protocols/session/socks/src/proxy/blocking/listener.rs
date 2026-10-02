//! Bounded blocking listener and complete per-connection handling.
// --- Standard library ---
use std::{
    io::ErrorKind::{Interrupted, WouldBlock},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

// --- Internal modules ---
use super::worker::Worker;
use crate::{error::Error, server::blocking::Server};

/// Run one complete TCP proxy connection: authenticate, authorize each resolved
/// target, dial, acknowledge the actual bound address, and relay both directions.
/// EOF half-closes only the opposite write side; an error terminates both halves.
/// Blocking DNS and caller callbacks cannot be interrupted by the I/O deadlines.
/// Accepts a validated blocking server; construction and policy are shared across versions.
///
/// # Errors
/// Returns a terminal session, target, timeout, or relay error.
pub fn serve_connection(stream: TcpStream, server: &Server) -> Result<(), Error> {
    let exchange = server.exchange(stream)?;
    let connection = exchange.authorize()?.connect()?;
    connection.relay(server.config.limits.relay)
}

/// Serve an owned TCP listener with bounded thread-per-session concurrency.
/// Set `shutdown` to stop acceptance and close both active relay sockets, then join
/// workers. Blocking DNS/callbacks and an in-progress bounded connection attempt
/// may delay shutdown. The listener is made
/// nonblocking; its socket options are not restored because it is consumed.
/// `on_error` reports individual session failures without stopping the listener.
/// It must not panic. Saturation leaves new connections in the OS backlog.
/// Sessions share the supplied server's validated configuration and policy.
/// Admission reserves two threads per session, bounded by both `limits.connections`
/// and half the server's thread budget. The calling thread is not part of that budget.
///
/// # Errors
/// Returns listener/thread creation errors or a worker panic.
pub fn serve(
    listener: TcpListener,
    server: &Server,
    shutdown: &AtomicBool,
    on_error: impl Fn(Error),
) -> Result<(), Error> {
    let mut listener = Listener::new(listener)?;
    let result = listener.run(server, shutdown, &on_error);
    // Always finish cleanup; a worker panic takes precedence over the accept-loop result.
    listener.shutdown(&on_error).and(result)
}

/// Owned listener and bounded set of active session workers.
struct Listener {
    socket: TcpListener,
    workers: Vec<Worker>,
}

impl Listener {
    /// Prepare nonblocking acceptance while leaving session I/O blocking.
    fn new(socket: TcpListener) -> Result<Self, Error> {
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            workers: Vec::new(),
        })
    }

    /// Reap finished sessions and accept new ones only while capacity is available.
    fn run(
        &mut self,
        server: &Server,
        shutdown: &AtomicBool,
        on_error: &impl Fn(Error),
    ) -> Result<(), Error> {
        // Set default timeout for park_timeout to avoid busy looping.
        let timeout = Duration::from_millis(10);

        // Main loop: reap finished sessions, accept new ones, and park if capacity is reached.
        while !shutdown.load(Ordering::Acquire) {
            self.reap(on_error)?;

            if self.workers.len() >= server.connection_capacity() {
                thread::park_timeout(timeout);
                continue;
            }

            match self.socket.accept() {
                Ok((stream, _)) => self.workers.push(Worker::spawn(stream, server)?),
                Err(e) if e.kind() == WouldBlock => thread::park_timeout(timeout),
                Err(e) if e.kind() == Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    /// Visit workers in reverse so swapped-in entries have already been checked.
    fn reap(&mut self, on_error: &impl Fn(Error)) -> Result<(), Error> {
        for idx in (0..self.workers.len()).rev() {
            if self.workers[idx].is_finished() {
                self.workers.swap_remove(idx).join(on_error)?;
            }
        }
        Ok(())
    }

    /// Close acceptance, cancel every session, then join all workers even after a panic.
    fn shutdown(self, on_error: &impl Fn(Error)) -> Result<(), Error> {
        drop(self.socket);

        let stopped = self
            .workers
            .iter()
            .map(Worker::shutdown)
            .fold(Ok(()), Result::and);

        let joined = self
            .workers
            .into_iter()
            .map(|worker| worker.join(on_error))
            .fold(Ok(()), Result::and);

        stopped.and(joined)
    }
}
