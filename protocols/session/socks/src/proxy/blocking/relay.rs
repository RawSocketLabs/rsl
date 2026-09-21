//! Blocking half-close-aware relay mechanics.
use crate::io::blocking::{Deadline, deadline};
use crate::{Connection, Stream, error::Error};
use std::{
    io,
    net::{Shutdown, TcpStream},
    thread,
    time::Duration,
};

fn copy_half(mut source: Stream<Deadline>, mut target: Deadline) -> io::Result<u64> {
    let result = io::copy(&mut source, &mut target);
    if result.is_err() {
        let _ = source.get_ref().stream.shutdown(Shutdown::Both);
        let _ = target.stream.shutdown(Shutdown::Both);
    } else {
        target.stream.shutdown(Shutdown::Write)?;
    }
    result
}

fn relay(client: Stream<TcpStream>, target: TcpStream, timeout: Duration) -> Result<(), Error> {
    let until = deadline(timeout)?;
    let (client, buffered) = client.into_parts();
    let client = Deadline {
        stream: client,
        deadline: until,
    };
    let client_read = client.stream.try_clone()?;
    let target_write = target.try_clone()?;
    thread::scope(|scope| {
        let outgoing = thread::Builder::new().spawn_scoped(scope, || {
            copy_half(
                Stream::from_parts(
                    Deadline {
                        stream: client_read,
                        deadline: until,
                    },
                    buffered,
                ),
                Deadline {
                    stream: target_write,
                    deadline: until,
                },
            )
        })?;
        let incoming = copy_half(
            Stream::from_parts(
                Deadline {
                    stream: target,
                    deadline: until,
                },
                bnb::BitBuf::new(),
            ),
            client,
        );
        let outgoing = outgoing.join().map_err(|_| Error::WorkerPanicked)?;
        outgoing?;
        incoming?;
        Ok(())
    })
}

impl Connection<TcpStream> {
    /// Relay both directions with half-close support and a total lifetime limit.
    /// An I/O failure or expired deadline terminates both sides.
    ///
    /// # Errors
    /// Returns invalid limits, I/O, timeout, or relay-worker errors.
    pub fn relay(self, timeout: Duration) -> Result<(), Error> {
        relay(self.client, self.target, timeout)
    }
}
