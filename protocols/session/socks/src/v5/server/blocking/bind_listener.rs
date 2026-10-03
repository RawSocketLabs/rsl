//! Readiness-backed, deadline-bounded acceptance for the blocking BIND helper.
// --- Standard library ---
use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    time::Instant,
};

// --- Workspace dependencies ---
use mio::{Events, Interest, Poll, Token};

// --- Internal modules ---
use crate::io::blocking::remaining;

/// Owns one nonblocking listener and its original poll until acceptance or failure.
pub(super) struct BindListener {
    listener: mio::net::TcpListener,
    poll: Poll,
    events: Events,
}
impl BindListener {
    /// Register an owned listener before any listening-success reply is sent.
    pub(super) fn new(listener: TcpListener) -> io::Result<Self> {
        listener.set_nonblocking(true)?;
        let mut listener = mio::net::TcpListener::from_std(listener);
        let poll = Poll::new()?;
        poll.registry()
            .register(&mut listener, Token(0), Interest::READABLE)?;
        Ok(Self {
            listener,
            poll,
            events: Events::with_capacity(4),
        })
    }

    /// Accept one peer before an absolute deadline, retrying interrupted waits and
    /// connections the peer aborted before acceptance.
    pub(super) fn accept(&mut self, until: Instant) -> io::Result<(TcpStream, SocketAddr)> {
        loop {
            remaining(until)?;
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    remaining(until)?;
                    let stream = TcpStream::from(stream);
                    stream.set_nonblocking(false)?;
                    return Ok((stream, peer));
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted
                    ) =>
                {
                    continue;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
            match self.poll.poll(&mut self.events, Some(remaining(until)?)) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                result => result?,
            }
        }
    }
}
