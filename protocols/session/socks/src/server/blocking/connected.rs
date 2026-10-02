//! Outbound connection established, with the client success reply still pending.
// --- Standard library ---
use std::net::TcpStream;

// --- Internal modules ---
use super::Exchange;
use crate::{Connection, error::Error, v5::Endpoint};

/// An authorized, connected target that has not been acknowledged to the client.
/// Inspect or register the socket before consuming this stage with `send_success`.
/// Dropping it closes owned sockets without sending success. Any socket handles cloned
/// by the caller remain that caller's responsibility. Delay while holding this stage
/// is caller-managed; sending either reply uses the configured bounded reply budget.
///
/// ```no_run
/// # use socks::{error::Error, server::blocking::Server};
/// # fn example(server: &Server, socket: std::net::TcpStream) -> Result<(), Error> {
/// let authorized = server.exchange(socket)?.authorize()?;
/// let connected = authorized.dial()?;
///
/// // Inspect or register connected.target() here, before acknowledging success.
/// // If local preparation fails, return connected.fail(error).
/// let connection = connected.send_success()?;
/// let (client, target) = connection.into_parts();
/// # let _ = (client, target);
/// # Ok(()) }
/// ```
///
/// A reply consumes the stage, preventing a second reply:
/// ```compile_fail,E0382
/// use socks::{error::Error, server::blocking::Connected};
/// fn twice(connected: Connected<'_>) -> Result<(), Error> {
///     let _connection = connected.send_success()?;
///     connected.send_success()?;
///     Ok(())
/// }
/// ```
#[must_use = "acknowledge the connection, fail it, or drop it to close owned sockets"]
pub struct Connected<'a> {
    pub(super) exchange: Exchange<'a>,
    pub(super) target: TcpStream,
    pub(super) bound: Endpoint,
}

impl Connected<'_> {
    /// Borrow the outbound socket for inspection or cancellation registration before success.
    /// Application relay has not started; avoid application I/O until acknowledgment.
    #[must_use]
    pub fn target(&self) -> &TcpStream {
        &self.target
    }

    /// Send success with the actual bound endpoint and return both relay sides.
    ///
    /// # Errors
    /// Returns a terminal reply error and closes owned sockets. No fallback failure
    /// reply is attempted after success writing starts, even if it only partially writes.
    pub fn send_success(self) -> Result<Connection<TcpStream>, Error> {
        Ok(Connection {
            client: self.exchange.succeed(self.bound)?,
            target: self.target,
        })
    }

    /// Close the target and attempt a bounded failure reply before returning the cause.
    ///
    /// # Errors
    /// Always returns `Err`: the supplied cause if the reply succeeds, otherwise the
    /// reply error. Consumes the stage so failure cannot be followed by success.
    pub fn fail(self, error: Error) -> Result<(), Error> {
        drop(self.target);
        self.exchange.fail(error)
    }
}
