use super::remaining;
use crate::error::Error;
use mio::net::TcpStream;
use std::{io, net::SocketAddr, time::Instant};

/// A nonblocking numeric TCP connection attempt with an absolute deadline.
/// Register `transport_mut()` as writable, then call `advance` on readiness (or
/// the deadline). No DNS or SOCKS negotiation occurs here.
pub struct Connector {
    stream: Option<TcpStream>,
    until: Instant,
    connected: bool,
}

impl Connector {
    /// Start one connection attempt. The same deadline can cover several candidates.
    ///
    /// # Errors
    /// Returns an expired deadline or immediate socket error.
    pub fn new(address: SocketAddr, until: Instant) -> Result<Self, Error> {
        remaining(until, Instant::now())?;
        Ok(Self {
            stream: Some(TcpStream::connect(address)?),
            until,
            connected: false,
        })
    }

    /// Borrow solely for registration/configuration; do not perform stream I/O yet.
    ///
    /// # Errors
    /// Returns `InvalidState` after error or handoff.
    pub fn transport_mut(&mut self) -> Result<&mut TcpStream, Error> {
        self.stream.as_mut().ok_or(Error::InvalidState)
    }

    /// Confirm connection with `take_error` and `peer_addr`, not readiness alone.
    /// `false` means wait for writable readiness again. Errors close the socket.
    ///
    /// # Errors
    /// Returns a terminal connection error or deadline expiry.
    pub fn advance(&mut self) -> Result<bool, Error> {
        let result = self.check();
        if result.is_err() {
            self.stream = None;
        }
        result
    }

    fn check(&mut self) -> Result<bool, Error> {
        remaining(self.until, Instant::now())?;
        let stream = self.stream.as_ref().ok_or(Error::InvalidState)?;
        if let Some(error) = stream.take_error()? {
            return Err(error.into());
        }
        match stream.peer_addr() {
            Ok(_) => {
                self.connected = true;
                Ok(true)
            }
            Err(error) if pending(&error) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// Transfer a confirmed connection once; returns `None` otherwise.
    pub fn take_stream(&mut self) -> Option<TcpStream> {
        if self.connected {
            self.stream.take()
        } else {
            None
        }
    }
}

fn pending(error: &io::Error) -> bool {
    if matches!(
        error.kind(),
        io::ErrorKind::NotConnected | io::ErrorKind::WouldBlock
    ) {
        return true;
    }
    // ErrorKind::InProgress is unstable at MSRV 1.85. Use platform constants,
    // not an error string or Linux's errno on every Unix target.
    #[cfg(unix)]
    if error.raw_os_error() == Some(libc::EINPROGRESS) {
        return true;
    }
    // WinSock WSAEINPROGRESS, as defined in the Windows SDK.
    #[cfg(windows)]
    if error.raw_os_error() == Some(10036) {
        return true;
    }
    false
}
