//! TCP BIND client with independent absolute handshake and peer-wait deadlines.
// --- Standard library ---
use std::{
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

// --- Internal modules ---
use super::{Binding, exchange::exchange};
use crate::{
    Stream,
    error::Error,
    io::blocking::{Deadline, deadline, remaining, unbounded},
    v5::{Command, Endpoint, auth::ClientAuth},
};

/// A TCP BIND listener reply with an already-running peer-wait budget.
/// Delaying `wait_for_peer` does not reset or postpone this budget.
pub struct TcpBinding {
    binding: Binding<Deadline>,
}

impl TcpBinding {
    /// The proxy's advertised listening endpoint.
    #[must_use]
    pub fn bound(&self) -> &Endpoint {
        self.binding.bound()
    }

    /// Absolute expiry of the second-reply budget, established after the first reply.
    #[must_use]
    pub fn deadline(&self) -> Instant {
        self.binding.stream.get_ref().deadline
    }

    /// Read the connecting-peer reply before the original deadline, then clear TCP timeouts.
    ///
    /// # Errors
    /// Expiry and protocol/transport failures close the owned TCP connection.
    pub fn wait_for_peer(self) -> Result<(Stream<TcpStream>, Endpoint), Error> {
        let until = self.deadline();
        remaining(until)?;
        // The read was bounded by the deadline; a reply already received is committed.
        let (stream, peer) = self.binding.wait_for_peer()?;
        Ok((unbounded(stream)?, peer))
    }
}

/// Connect, authenticate, and receive the listener reply within one absolute budget.
/// A separate peer-wait budget starts immediately after that reply succeeds.
///
/// ```no_run
/// use socks::v5::{auth::ClientAuth, client::blocking::bind_tcp};
/// use std::{net::SocketAddr, time::Duration};
/// # fn example() -> Result<(), socks::error::Error> {
/// // Ask the proxy to listen, with separate setup and peer-wait budgets.
/// let binding = bind_tcp(
///     SocketAddr::from(([127, 0, 0, 1], 1080)),
///     SocketAddr::from(([192, 0, 2, 1], 0)).into(),
///     ClientAuth::NoAuthentication,
///     Duration::from_secs(5), Duration::from_secs(30),
/// )?;
///
/// // Notify the remote of this address over your control channel.
/// let advertised = binding.bound().clone();
///
/// // That notification consumes time from binding.deadline(); waiting does not reset it.
/// let (stream, peer) = binding.wait_for_peer()?;
/// # let _ = (advertised, stream, peer);
/// # Ok(()) }
/// ```
///
/// # Errors
/// Invalid arguments fail before connecting; all subsequent failures close the socket.
pub fn bind_tcp(
    proxy: SocketAddr,
    expected_peer: Endpoint,
    auth: ClientAuth<'_>,
    handshake_timeout: Duration,
    peer_timeout: Duration,
) -> Result<TcpBinding, Error> {
    expected_peer.validate_structure()?;
    let credentials = auth.credentials()?;
    deadline(peer_timeout)?;
    let until = deadline(handshake_timeout)?;
    let socket = TcpStream::connect_timeout(&proxy, remaining(until)?)?;
    let (stream, bound) = exchange(
        Deadline {
            stream: socket,
            deadline: until,
        },
        expected_peer,
        Command::Bind,
        auth.method(),
        credentials,
    )?;
    let mut binding = Binding { stream, bound };
    remaining(until)?;
    binding.stream.get_mut().deadline = deadline(peer_timeout)?;
    Ok(TcpBinding { binding })
}

#[cfg(test)]
mod unit {
    // --- Internal modules ---
    use super::*;

    #[test]
    fn expired_peer_budget_rejects_even_a_prefetched_success() {
        // --- Standard library ---
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (control, _) = listener.accept().unwrap();
        let reply = [5, 0, 0, 1, 127, 0, 0, 1, 1, 187];
        peer.write_all(&[&[5, 0][..], &reply, &reply, b"early"].concat())
            .unwrap();
        let mut binding = super::super::bind(
            Deadline {
                stream: control,
                deadline: deadline(Duration::from_secs(5)).unwrap(),
            },
            SocketAddr::from(([127, 0, 0, 1], 0)).into(),
            ClientAuth::NoAuthentication,
        )
        .unwrap();
        // Inject an already-expired deadline: even complete prefetched bytes cannot
        // reset it. No sleep or wall-clock tolerance is part of this assertion.
        binding.stream.get_mut().deadline = Instant::now();
        assert!(matches!(
            (TcpBinding { binding }).wait_for_peer(),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut
        ));
    }
}
