//! Resumable SOCKS5 client handshake.
// --- Standard library ---
use std::io::{Read, Write};

// --- Workspace dependencies ---
use mio::Interest;

// --- Internal modules ---
use crate::io::stream::MAX_FRAME_LEN;
use crate::v5::mio_io::Io;
use crate::v5::sansio::{self, Step};
use crate::{
    Stream,
    error::Error,
    v5::{AuthMethod, Command, Endpoint, UsernamePasswordRequest, auth::ClientAuth},
};

/// A resumable client handshake. No I/O occurs during construction.
///
/// `advance` stops only on completion, `WouldBlock`, or terminal error. Retrying
/// after `WouldBlock` never resends accepted output. The caller supplies deadlines
/// and readiness scheduling. Credentials and buffers are not zeroized on drop.
pub struct Client<S> {
    io: Io<S>,
    machine: sansio::Client,
    /// The current message is fully written but the transport has not been flushed.
    unflushed: bool,
    established: bool,
}

impl<S: Read + Write> Client<S> {
    /// Validate a destination and authentication before any transport writes.
    /// Domains are forwarded unchanged to the proxy, not resolved locally.
    ///
    /// # Errors
    /// Returns invalid credential, endpoint, or codec errors.
    pub fn new(stream: S, dest: Endpoint, auth: ClientAuth<'_>) -> Result<Self, Error> {
        dest.validate_destination()?;
        let credentials = auth.credentials()?;
        Self::prepared(stream, dest, auth.method(), credentials)
    }

    pub(super) fn prepared(
        stream: S,
        dest: Endpoint,
        method: AuthMethod,
        credentials: Option<UsernamePasswordRequest>,
    ) -> Result<Self, Error> {
        let machine = sansio::Client::prepared(dest, Command::Connect, method, credentials)?;
        let mut io = Io::new(stream);
        io.interest = Interest::WRITABLE;
        Ok(Self {
            io,
            machine,
            unflushed: false,
            established: false,
        })
    }

    /// Required readiness after `advance` reports incomplete progress.
    pub fn interest(&self) -> Option<Interest> {
        (self.io.is_open() && !self.established).then_some(self.io.interest)
    }

    /// Call `advance` again without waiting for readiness after a fairness yield.
    /// An event loop must queue this continuation or use a zero poll timeout.
    pub fn needs_advance(&self) -> bool {
        self.io.again
    }

    /// Borrow the transport for Mio registration/configuration, not application I/O.
    ///
    /// # Errors
    /// Returns `InvalidState` after failure or handoff.
    pub fn transport_mut(&mut self) -> Result<&mut S, Error> {
        Ok(self.io.stream()?.get_mut())
    }

    /// Advance until blocked or established. `true` means handoff is available.
    ///
    /// # Errors
    /// All errors are terminal and drop the owned transport. Borrowed transports
    /// must be closed by their owner. `WouldBlock` is never returned as an error.
    pub fn advance(&mut self) -> Result<bool, Error> {
        self.io.begin();
        let result = self.drive();
        if result.is_err() {
            self.io.close();
        }
        result
    }

    /// Move bytes between the transport and the machine until blocked or established.
    fn drive(&mut self) -> Result<bool, Error> {
        self.io.stream()?;
        let mut scratch = [0; MAX_FRAME_LEN];
        loop {
            if self.unflushed {
                if !self.io.flush_transport()? {
                    return Ok(false);
                }
                self.unflushed = false;
            }
            match self.machine.advance()? {
                Step::Transmit => {
                    let Some(count) = self.io.write_bytes(self.machine.transmit())? else {
                        return Ok(false);
                    };
                    self.machine.sent(count);
                    self.unflushed = self.machine.transmit().is_empty();
                }
                Step::Receive => {
                    let limit = self.machine.receive_limit();
                    let Some(count) = self.io.read_bytes(&mut scratch[..limit])? else {
                        return Ok(false);
                    };
                    match count {
                        0 => self.machine.receive_eof(),
                        count => self.machine.receive(&scratch[..count])?,
                    }
                }
                Step::Established => {
                    self.established = true;
                    return Ok(true);
                }
            }
        }
    }

    /// Take the established stream and bound endpoint once, preserving read-ahead.
    /// Returns `None` before completion, after failure, or after a previous handoff.
    pub fn take_stream(&mut self) -> Option<(Stream<S>, Endpoint)> {
        let (input, bound) = self.machine.finish().ok()?;
        let (transport, unused) = self.io.take()?.into_parts();
        debug_assert!(
            unused.is_empty(),
            "handshake reads bypass the stream buffer"
        );
        Some((Stream::from_parts(transport, input), bound))
    }
}
