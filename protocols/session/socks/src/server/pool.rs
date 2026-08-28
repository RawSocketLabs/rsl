//! Concurrency and accept-timeout primitives shared by every version's server.
//!
//! Neither type is SOCKS-specific; they are the resource-exhaustion guards the
//! [`Server`](crate::server) types lean on, factored out so the SOCKS4 and
//! SOCKS5 proxies share one implementation.

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// A counting semaphore: a server acquires a permit before spawning a handler
/// and the permit is released when the handler thread ends, so a flood of
/// connections cannot spawn unbounded threads — excess connections wait in the
/// OS accept backlog until a slot frees.
pub(crate) struct Semaphore {
    permits: Mutex<usize>,
    available: Condvar,
}

impl Semaphore {
    pub(crate) fn new(permits: usize) -> Arc<Self> {
        Arc::new(Self {
            permits: Mutex::new(permits),
            available: Condvar::new(),
        })
    }

    /// Blocks until a permit is available, then takes it.
    pub(crate) fn acquire(self: &Arc<Self>) -> Permit {
        let mut permits = self.permits.lock().unwrap();
        while *permits == 0 {
            permits = self.available.wait(permits).unwrap();
        }
        *permits -= 1;
        Permit(Arc::clone(self))
    }
}

/// Returns a permit to the semaphore when dropped.
pub(crate) struct Permit(Arc<Semaphore>);

impl Drop for Permit {
    fn drop(&mut self) {
        *self.0.permits.lock().unwrap() += 1;
        self.0.available.notify_one();
    }
}

/// Accept one connection, optionally bounded by a deadline. Without a timeout
/// it blocks; with one it polls the non-blocking listener until a peer arrives
/// or the deadline passes, returning a `TimedOut` error so a BIND whose peer
/// never connects cannot hold the handler (and its listener) open forever.
pub(crate) fn accept_with_timeout(
    listener: &TcpListener,
    timeout: Option<Duration>,
) -> io::Result<(TcpStream, SocketAddr)> {
    let Some(timeout) = timeout else {
        return listener.accept();
    };
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + timeout;
    let result = loop {
        match listener.accept() {
            Ok(accepted) => break Ok(accepted),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    break Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "BIND peer-wait timed out",
                    ));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => break Err(err),
        }
    };
    // Restore blocking mode so the relay then reads normally.
    if let Ok((conn, _)) = &result {
        conn.set_nonblocking(false)?;
    }
    result
}

#[cfg(test)]
mod unit {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn semaphore_blocks_at_capacity_and_releases_on_drop() {
        let sem = Semaphore::new(1);
        let held = sem.acquire(); // capacity exhausted

        let (tx, rx) = mpsc::channel();
        let waiter = {
            let sem = Arc::clone(&sem);
            thread::spawn(move || {
                let _permit = sem.acquire(); // must block until `held` drops
                tx.send(()).unwrap();
            })
        };

        // The waiter cannot acquire while the only permit is held.
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "acquire should block while at capacity"
        );

        drop(held); // frees the permit
        assert!(
            rx.recv_timeout(Duration::from_secs(2)).is_ok(),
            "acquire should unblock once a permit is released"
        );
        waiter.join().unwrap();
    }
}
