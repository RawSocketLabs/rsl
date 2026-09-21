//! Bounded blocking listener and complete per-connection handling.
use crate::{Server, error::Error};
use std::{
    io,
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

/// Run one complete TCP proxy connection: authenticate, authorize each resolved
/// target, dial, acknowledge the actual bound address, and relay both directions.
/// EOF half-closes only the opposite write side; an error terminates both halves.
/// Blocking DNS and caller callbacks cannot be interrupted by the I/O deadlines.
/// Accepts the validated server returned by [`Server::builder`] or [`Server::new`].
///
/// # Errors
/// Returns a terminal session, target, timeout, or relay error.
pub fn serve_connection(stream: TcpStream, server: &Server) -> Result<(), Error> {
    serve_connection_with(stream, server, |_| Ok(()))
}

fn serve_connection_with(
    stream: TcpStream,
    server: &Server,
    register_target: impl FnOnce(&TcpStream) -> Result<(), Error>,
) -> Result<(), Error> {
    let exchange = server.exchange(stream)?;
    let connection = exchange.authorize()?.connect_with(register_target)?;
    connection.relay(server.config.limits.relay)
}

// Listener shutdown must close both relay sockets. The cancellation bit also
// covers shutdown racing a connect that has not registered its target yet.
struct Sockets {
    client: TcpStream,
    target: Option<TcpStream>,
    cancelled: bool,
}

impl Sockets {
    fn register(&mut self, target: &TcpStream) -> Result<(), Error> {
        if self.cancelled {
            target.shutdown(Shutdown::Both)?;
            return Err(io::Error::from(io::ErrorKind::Interrupted).into());
        }
        self.target = Some(target.try_clone()?);
        Ok(())
    }

    fn shutdown(&mut self) {
        self.cancelled = true;
        let _ = self.client.shutdown(Shutdown::Both);
        if let Some(target) = &self.target {
            let _ = target.shutdown(Shutdown::Both);
        }
    }
}

/// Serve an owned TCP listener with bounded thread-per-session concurrency.
/// Set `shutdown` to stop acceptance and close both active relay sockets, then join
/// workers. Blocking DNS/callbacks and an in-progress bounded connection attempt
/// may delay shutdown. The listener is made
/// nonblocking; its socket options are not restored because it is consumed.
/// `on_error` reports individual session failures without stopping the listener.
/// It must not panic. Saturation leaves new connections in the OS backlog.
/// Sessions share the supplied server's validated configuration and policy.
///
/// # Errors
/// Returns listener/thread creation errors or a worker panic.
pub fn serve(
    listener: TcpListener,
    server: &Server,
    shutdown: &AtomicBool,
    on_error: impl Fn(Error),
) -> Result<(), Error> {
    listener.set_nonblocking(true)?;
    let mut workers = Vec::<(Arc<Mutex<Sockets>>, thread::JoinHandle<Result<(), Error>>)>::new();
    let result = (|| {
        while !shutdown.load(Ordering::Acquire) {
            let mut index = 0;
            while index < workers.len() {
                if workers[index].1.is_finished() {
                    let (_, worker) = workers.swap_remove(index);
                    if let Err(error) = worker.join().map_err(|_| Error::WorkerPanicked)? {
                        on_error(error);
                    }
                } else {
                    index += 1;
                }
            }
            if workers.len() < server.config.limits.connections {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false)?;
                        let control = Arc::new(Mutex::new(Sockets {
                            client: stream.try_clone()?,
                            target: None,
                            cancelled: false,
                        }));
                        let registration = Arc::clone(&control);
                        let server = server.clone();
                        let worker = thread::Builder::new().spawn(move || {
                            serve_connection_with(stream, &server, |target| {
                                registration
                                    .lock()
                                    .map_err(|_| Error::WorkerPanicked)?
                                    .register(target)
                            })
                        })?;
                        workers.push((control, worker));
                        continue;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            thread::park_timeout(Duration::from_millis(10));
        }
        Ok(())
    })();
    drop(listener);
    let mut result = result;
    for (control, _) in &workers {
        match control.lock() {
            Ok(mut sockets) => sockets.shutdown(),
            Err(_) => result = Err(Error::WorkerPanicked),
        }
    }
    for (_, worker) in workers {
        match worker.join() {
            Ok(Err(error)) => on_error(error),
            Err(_) => result = Err(Error::WorkerPanicked),
            Ok(Ok(())) => {}
        }
    }
    result
}
