//! Resumable SOCKS5 client handshake.
use crate::v5::mio_io::Io;
use crate::{
    Stream,
    error::Error,
    v5::{
        AuthMethod, Command, Endpoint, MethodRequest, MethodSelection, Reply, Request,
        UsernamePasswordRequest, UsernamePasswordResponse, VERSION, auth::ClientAuth,
    },
};
use mio::Interest;
use std::io::{Read, Write};

enum Phase {
    Greeting,
    Selection,
    Credentials,
    Authentication,
    Request,
    Reply,
    Complete,
}

/// A resumable client handshake. No I/O occurs during construction.
///
/// `advance` stops only on completion, `WouldBlock`, or terminal error. Retrying
/// after `WouldBlock` never resends accepted output. The caller supplies deadlines
/// and readiness scheduling. Credentials and buffers are not zeroized on drop.
pub struct Client<S> {
    io: Io<S>,
    phase: Phase,
    method: AuthMethod,
    credentials: Option<UsernamePasswordRequest>,
    destination: Endpoint,
    bound: Option<Endpoint>,
}

impl<S: Read + Write> Client<S> {
    /// Validate a destination and authentication before any transport writes.
    /// Domains are forwarded unchanged to the proxy, not resolved locally.
    ///
    /// # Errors
    /// Returns invalid credential, endpoint, or codec errors.
    pub fn new(stream: S, destination: Endpoint, auth: ClientAuth<'_>) -> Result<Self, Error> {
        destination.validate_connect_destination()?;
        let credentials = auth.credentials()?;
        Self::prepared(stream, destination, auth.method(), credentials)
    }

    pub(super) fn prepared(
        stream: S,
        destination: Endpoint,
        method: AuthMethod,
        credentials: Option<UsernamePasswordRequest>,
    ) -> Result<Self, Error> {
        let mut io = Io::new(stream);
        io.queue(&MethodRequest {
            version: VERSION,
            methods: vec![method],
        })?;
        Ok(Self {
            io,
            phase: Phase::Greeting,
            method,
            credentials,
            destination,
            bound: None,
        })
    }

    /// Required readiness after `advance` reports incomplete progress.
    pub fn interest(&self) -> Option<Interest> {
        (self.io.is_open() && !matches!(self.phase, Phase::Complete)).then_some(self.io.interest)
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

    fn drive(&mut self) -> Result<bool, Error> {
        self.io.stream()?;
        loop {
            match self.phase {
                Phase::Greeting => {
                    if !self.io.flush()? {
                        return Ok(false);
                    }
                    self.phase = Phase::Selection;
                }
                Phase::Selection => {
                    let Some(message) = self.io.receive::<MethodSelection>()? else {
                        return Ok(false);
                    };
                    message.check_offered(self.method)?;
                    if let Some(credentials) = self.credentials.take() {
                        self.io.queue(&credentials)?;
                        self.phase = Phase::Credentials;
                    } else {
                        self.queue_request()?;
                    }
                }
                Phase::Credentials => {
                    if !self.io.flush()? {
                        return Ok(false);
                    }
                    self.phase = Phase::Authentication;
                }
                Phase::Authentication => {
                    let Some(message) = self.io.receive::<UsernamePasswordResponse>()? else {
                        return Ok(false);
                    };
                    message.ensure_success()?;
                    self.queue_request()?;
                }
                Phase::Request => {
                    if !self.io.flush()? {
                        return Ok(false);
                    }
                    self.phase = Phase::Reply;
                }
                Phase::Reply => {
                    let Some(message) = self.io.command::<Reply>()? else {
                        return Ok(false);
                    };
                    message.ensure_success()?;
                    self.bound = Some(message.bound);
                    self.phase = Phase::Complete;
                }
                Phase::Complete => return Ok(true),
            }
        }
    }

    fn queue_request(&mut self) -> Result<(), Error> {
        self.io.queue(&Request {
            version: VERSION,
            reserved: 0,
            command: Command::Connect,
            destination: self.destination.clone(),
        })?;
        self.phase = Phase::Request;
        Ok(())
    }

    /// Take the established stream and bound endpoint once, preserving read-ahead.
    /// Returns `None` before completion, after failure, or after a previous handoff.
    pub fn take_stream(&mut self) -> Option<(Stream<S>, Endpoint)> {
        if !matches!(self.phase, Phase::Complete) {
            return None;
        }
        Some((self.io.take()?, self.bound.take()?))
    }
}
