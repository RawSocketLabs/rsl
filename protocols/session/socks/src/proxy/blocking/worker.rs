//! One blocking session thread and its cancellation handles.
// --- Standard library ---
use std::{
    net::TcpStream,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

// --- Internal modules ---
use super::sockets::Sockets;
use crate::{error::Error, server::blocking::Server};

/// Keep a worker's join handle paired with the sockets needed to stop it.
pub(super) struct Worker {
    sockets: Arc<Mutex<Sockets>>,
    thread: JoinHandle<Result<(), Error>>,
}

impl Worker {
    /// Start a session only after retaining its client-side cancellation handle.
    pub(super) fn spawn(stream: TcpStream, server: &Server) -> Result<Self, Error> {
        stream.set_nonblocking(false)?;
        let sockets = Arc::new(Mutex::new(Sockets::new(stream.try_clone()?)));
        let registration = Arc::clone(&sockets);
        let server = server.clone();
        let thread = thread::Builder::new().spawn(move || {
            let authorized = server.exchange(stream)?.authorize()?;
            let connected = authorized.dial()?;

            // Release the guard before replying; shutdown must not wait on reply I/O.
            let registered = match registration.lock() {
                Ok(mut sockets) => sockets.register(connected.target()),
                Err(_) => Err(Error::WorkerPanicked),
            };
            if let Err(error) = registered {
                return connected.fail(error);
            }

            let connection = connected.send_success()?;
            connection.relay(server.config.limits.relay)
        })?;
        Ok(Self { sockets, thread })
    }

    /// Whether joining can complete without waiting for this session.
    pub(super) fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Interrupt both sides before the listener joins any remaining workers.
    pub(super) fn shutdown(&self) -> Result<(), Error> {
        self.sockets
            .lock()
            .map_err(|_| Error::WorkerPanicked)?
            .shutdown();
        Ok(())
    }

    /// Report session errors through the callback; a thread panic terminates the listener.
    pub(super) fn join(self, on_error: &impl Fn(Error)) -> Result<(), Error> {
        if let Err(error) = self.thread.join().map_err(|_| Error::WorkerPanicked)? {
            on_error(error);
        }
        Ok(())
    }
}
