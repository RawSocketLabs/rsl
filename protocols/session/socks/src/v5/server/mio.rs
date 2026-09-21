//! Resumable SOCKS5 embedded server exchange.
use crate::v5::mio_io::Io;
use crate::{
    Stream,
    error::Error,
    server::policy::ServerAuth,
    v5::auth,
    v5::{
        AuthMethod, Endpoint, MethodSelection, Reply, ReplyCode, Request, UsernamePasswordResponse,
        UsernamePasswordStatus, VERSION,
    },
};
use mio::Interest;
use std::io::{Read, Write};

/// Observable progress of an embedded server exchange.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    /// Negotiation or a reply is awaiting transport readiness/continuation.
    Exchanging,
    /// Authentication and CONNECT parsing succeeded; authorize and dial externally.
    Requested,
    /// The success reply is fully flushed; lossless handoff is available.
    Established,
    /// The explicitly requested failure reply was flushed and the socket closed.
    Rejected,
}

enum Phase {
    Greeting,
    Selection,
    Credentials,
    Authentication,
    Request,
    Requested,
    Success,
    Failure,
    Established,
    Rejected,
    Failed,
}

/// A resumable embedded server handshake; performs no DNS, authorization, or dial.
///
/// Drive immediately and on requested readiness. `needs_advance()` requires an
/// immediate continuation after a fairness yield. Authentication executes once.
/// Caller-managed deadlines must also cover queued replies and policy/dial work.
pub struct Exchange<S> {
    io: Io<S>,
    auth: ServerAuth,
    phase: Phase,
    destination: Option<Endpoint>,
    failure: Option<Error>,
}

impl<S: Read + Write> Exchange<S> {
    /// Wrap an already connected nonblocking transport with explicit authentication.
    pub fn new(stream: S, auth: ServerAuth) -> Self {
        Self {
            io: Io::new(stream),
            auth,
            phase: Phase::Greeting,
            destination: None,
            failure: None,
        }
    }

    /// Required readiness; `None` while awaiting the caller's policy/dial decision
    /// or after completion. Register/reregister only when an interest is present.
    pub fn interest(&self) -> Option<Interest> {
        (self.io.is_open()
            && !matches!(
                self.phase,
                Phase::Requested | Phase::Established | Phase::Rejected
            ))
        .then_some(self.io.interest)
    }

    /// Whether a fairness yield requires advancing without another readiness edge.
    pub fn needs_advance(&self) -> bool {
        self.io.again
    }

    /// Authentication completed once the request is available. No credentials are retained here.
    pub fn authentication(&self) -> Option<crate::server::policy::Authentication> {
        self.destination.as_ref().map(|_| {
            if self.auth.requires_credentials() {
                crate::server::policy::Authentication::UsernamePassword
            } else {
                crate::server::policy::Authentication::Unauthenticated
            }
        })
    }

    /// Original destination once authentication and request validation succeed.
    pub fn destination(&self) -> Option<&Endpoint> {
        self.destination.as_ref()
    }

    /// Borrow for readiness registration/configuration only, not stream reads/writes.
    ///
    /// # Errors
    /// Returns `InvalidState` after terminal error, rejection, or handoff.
    pub fn transport_mut(&mut self) -> Result<&mut S, Error> {
        Ok(self.io.stream()?.get_mut())
    }

    /// Queue success after the caller authorizes and connects the target. Supply
    /// its actual local endpoint. Advance until `Established`, then transfer the
    /// stream and keep its original Mio poll alive. Registration with a different
    /// poll is not portable, even after deregistration.
    ///
    /// # Errors
    /// Rejects calls outside `Requested` and invalid bound endpoints.
    pub fn send_success(&mut self, bound: Endpoint) -> Result<(), Error> {
        if !matches!(self.phase, Phase::Requested) {
            return Err(Error::InvalidState);
        }
        self.io.queue(&Reply::success(bound)?)?;
        self.phase = Phase::Success;
        Ok(())
    }

    /// Queue a failing reply, then advance until `Rejected`. Never sends success.
    ///
    /// # Errors
    /// Rejects calls outside `Requested`, success codes, and encoding failures.
    pub fn send_failure(&mut self, code: ReplyCode) -> Result<(), Error> {
        if !matches!(self.phase, Phase::Requested) {
            return Err(Error::InvalidState);
        }
        self.io.queue(&Reply::failure(code)?)?;
        self.phase = Phase::Failure;
        Ok(())
    }

    pub(crate) fn fail(&mut self, error: Error) -> Result<(), Error> {
        self.send_failure(error.reply_code())?;
        self.failure = Some(error);
        Ok(())
    }

    /// Advance until blocked, awaiting policy, or complete.
    ///
    /// # Errors
    /// Errors are terminal and drop owned transports. Protocol refusals flush
    /// their failure reply before returning the cause; a write error takes
    /// precedence. `WouldBlock` instead returns `Exchanging` without losing state.
    pub fn advance(&mut self) -> Result<Stage, Error> {
        self.io.begin();
        if matches!(self.phase, Phase::Rejected) {
            return Ok(Stage::Rejected);
        }
        let result = self.drive();
        if result.is_err() {
            self.io.close();
            self.phase = Phase::Failed;
        }
        result
    }

    fn drive(&mut self) -> Result<Stage, Error> {
        self.io.stream()?;
        loop {
            match self.phase {
                Phase::Greeting => {
                    let Some(offer) = self.io.receive::<crate::v5::MethodRequest>()? else {
                        return Ok(Stage::Exchanging);
                    };
                    if offer.version != VERSION {
                        return Err(Error::VersionNotAccepted(offer.version));
                    }
                    let selected = auth::select(&self.auth, &offer);
                    self.io.queue(&MethodSelection {
                        version: VERSION,
                        method: selected
                            .as_ref()
                            .copied()
                            .unwrap_or(AuthMethod::NoAcceptable),
                    })?;
                    self.failure = selected.err();
                    self.phase = Phase::Selection;
                }
                Phase::Selection => {
                    if !self.io.flush()? {
                        return Ok(Stage::Exchanging);
                    }
                    if let Some(error) = self.failure.take() {
                        return Err(error);
                    }
                    self.phase = if auth::server_method(&self.auth) == AuthMethod::UsernamePassword
                    {
                        Phase::Credentials
                    } else {
                        Phase::Request
                    };
                }
                Phase::Credentials => {
                    let Some(credentials) = self.io.receive()? else {
                        return Ok(Stage::Exchanging);
                    };
                    let accepted = auth::authenticate(&self.auth, &credentials);
                    self.io.queue(&UsernamePasswordResponse {
                        version: 1,
                        status: UsernamePasswordStatus::from(u8::from(!accepted)),
                    })?;
                    if !accepted {
                        self.failure = Some(Error::AuthenticationRejected);
                    }
                    self.phase = Phase::Authentication;
                }
                Phase::Authentication => {
                    if !self.io.flush()? {
                        return Ok(Stage::Exchanging);
                    }
                    if let Some(error) = self.failure.take() {
                        return Err(error);
                    }
                    self.phase = Phase::Request;
                }
                Phase::Request => {
                    let result = self.io.command::<Request>().and_then(|request| {
                        if let Some(value) = &request {
                            super::validation::check_request(value)?;
                        }
                        Ok(request)
                    });
                    match result {
                        Ok(Some(request)) => {
                            self.destination = Some(request.destination);
                            self.phase = Phase::Requested;
                        }
                        Ok(None) => return Ok(Stage::Exchanging),
                        Err(error) => {
                            self.io.queue(&Reply::failure(error.reply_code())?)?;
                            self.failure = Some(error);
                            self.phase = Phase::Failure;
                        }
                    }
                }
                Phase::Requested => return Ok(Stage::Requested),
                Phase::Success => {
                    if !self.io.flush()? {
                        return Ok(Stage::Exchanging);
                    }
                    self.phase = Phase::Established;
                }
                Phase::Failure => {
                    if !self.io.flush()? {
                        return Ok(Stage::Exchanging);
                    }
                    self.io.close();
                    self.phase = Phase::Rejected;
                    if let Some(error) = self.failure.take() {
                        return Err(error);
                    }
                }
                Phase::Established => return Ok(Stage::Established),
                Phase::Rejected => return Ok(Stage::Rejected),
                Phase::Failed => return Err(Error::InvalidState),
            }
        }
    }

    /// Take an established stream once, including all prefetched application bytes.
    /// Never extracts a partial handshake. `None` leaves the session unchanged.
    pub fn take_stream(&mut self) -> Option<Stream<S>> {
        if matches!(self.phase, Phase::Established) {
            self.io.take()
        } else {
            None
        }
    }
}
