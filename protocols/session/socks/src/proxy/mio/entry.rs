use super::{
    relay::{Prepared, Relay},
    resolver::Resolver,
};
use crate::{
    error::Error,
    io::mio::{Connector, deadline, remaining},
    server::ServerConfig,
    v5::server::mio::{Exchange, Stage},
};
use mio::{Interest, Token, net::TcpStream};
use std::{
    collections::VecDeque,
    net::SocketAddr,
    time::{Duration, Instant},
};

enum State {
    Exchange,
    Resolving,
    Connecting {
        connector: Option<Connector>,
        addresses: VecDeque<SocketAddr>,
        last: Option<Error>,
    },
    Reply {
        target: Option<Prepared>,
    },
    Relay(Relay),
    Complete,
}

pub(super) struct Entry {
    exchange: Option<Exchange<TcpStream>>,
    peer: SocketAddr,
    request: Option<crate::server::policy::RequestInfo>,
    state: State,
    pub(super) until: Instant,
    pub(super) client_interest: Option<Interest>,
    pub(super) target_interest: Option<Interest>,
    pub(super) target_token: Option<Token>,
    pub(super) target_replaced: bool,
    pub(super) target_ready: bool,
    pub(super) queued: bool,
    resolved: Option<Result<Vec<SocketAddr>, Error>>,
    id: usize,
}

impl Entry {
    pub(super) fn new(
        id: usize,
        stream: TcpStream,
        peer: SocketAddr,
        config: &ServerConfig,
    ) -> Result<Self, Error> {
        let exchange = match config.protocol() {
            crate::Version::V5 => Exchange::new(stream, config.policy.authentication().clone()),
        };
        Ok(Self {
            exchange: Some(exchange),
            peer,
            request: None,
            state: State::Exchange,
            until: deadline(Instant::now(), config.limits.handshake)?,
            client_interest: None,
            target_interest: None,
            target_token: None,
            target_replaced: false,
            target_ready: false,
            queued: false,
            resolved: None,
            id,
        })
    }

    // Late answers must not revive an entry that has already left resolution.
    pub(super) fn accept_resolution(&mut self, addresses: Result<Vec<SocketAddr>, Error>) -> bool {
        if matches!(self.state, State::Resolving) {
            self.resolved = Some(addresses);
            true
        } else {
            false
        }
    }

    fn exchange(&mut self) -> Result<&mut Exchange<TcpStream>, Error> {
        self.exchange.as_mut().ok_or(Error::InvalidState)
    }
    pub(super) fn complete(&self) -> bool {
        matches!(self.state, State::Complete)
    }

    fn fail(&mut self, error: Error, config: &ServerConfig) -> Result<bool, Error> {
        self.exchange()?.fail(error)?;
        self.state = State::Reply { target: None };
        self.until = deadline(
            Instant::now(),
            config.limits.handshake.min(Duration::from_secs(10)),
        )?;
        // Any previous target has been dropped; retire its token before reuse.
        self.target_replaced = true;
        Ok(true)
    }

    fn authorize(
        &mut self,
        addresses: Result<Vec<SocketAddr>, Error>,
        config: &ServerConfig,
    ) -> Result<bool, Error> {
        let result = addresses.and_then(|addresses| {
            let peer = self.peer;
            let request = self.request.as_ref().ok_or(Error::InvalidState)?;
            config.policy.authorize_addresses(peer, request, addresses)
        });
        // Callback elapsed time counts, including when it denies every candidate.
        let result = remaining(self.until, Instant::now()).and(result);
        match result {
            Ok(addresses) => {
                self.state = State::Connecting {
                    connector: None,
                    addresses: addresses.into(),
                    last: None,
                };
                Ok(true)
            }
            Err(error) => self.fail(error, config),
        }
    }

    pub(super) fn advance(
        &mut self,
        config: &ServerConfig,
        resolver: &Resolver,
    ) -> Result<bool, Error> {
        if let Err(error) = remaining(self.until, Instant::now()) {
            return if matches!(self.state, State::Resolving | State::Connecting { .. }) {
                self.fail(error, config)
            } else {
                Err(error)
            };
        }
        match &mut self.state {
            State::Exchange => {
                let stage = self.exchange()?.advance()?;
                remaining(self.until, Instant::now())?;
                match stage {
                    Stage::Requested => {
                        self.until = deadline(Instant::now(), config.limits.connect)?;
                        let destination = self
                            .exchange()?
                            .destination()
                            .ok_or(Error::InvalidState)?
                            .clone();
                        let destination: crate::Destination = destination.into();
                        self.request = Some(crate::server::policy::RequestInfo {
                            version: config.protocol(),
                            authentication: self
                                .exchange()?
                                .authentication()
                                .ok_or(Error::InvalidState)?,
                            operation: crate::server::policy::Operation::Connect,
                            destination,
                        });
                        let destination = &self
                            .request
                            .as_ref()
                            .ok_or(Error::InvalidState)?
                            .destination;
                        if let Some(address) = destination.socket_addr() {
                            self.authorize(Ok(vec![address]), config)
                        } else {
                            match resolver.submit(self.id, destination.clone()) {
                                Ok(()) => {
                                    self.state = State::Resolving;
                                    Ok(false)
                                }
                                Err(error) => self.fail(error, config),
                            }
                        }
                    }
                    _ => Ok(self.exchange()?.needs_advance()),
                }
            }
            State::Resolving => {
                if let Some(result) = self.resolved.take() {
                    self.authorize(result, config)
                } else {
                    Ok(false)
                }
            }
            State::Connecting { .. } => self.connect(config),
            State::Reply { .. } => {
                let stage = self.exchange()?.advance()?;
                remaining(self.until, Instant::now())?;
                match stage {
                    Stage::Established => {
                        let client = self.exchange()?.take_stream().ok_or(Error::InvalidState)?;
                        let State::Reply {
                            target: Some(target),
                        } = std::mem::replace(&mut self.state, State::Complete)
                        else {
                            return Err(Error::InvalidState);
                        };
                        self.state = State::Relay(target.into_relay(client));
                        self.exchange = None;
                        self.until = deadline(Instant::now(), config.limits.relay)?;
                        Ok(true) // Drain any buffered prefix without waiting for a socket edge.
                    }
                    Stage::Rejected => {
                        self.state = State::Complete;
                        Ok(false)
                    }
                    _ => Ok(self.exchange()?.needs_advance()),
                }
            }
            State::Relay(relay) => {
                let again = relay.advance()?;
                if relay.complete() {
                    self.state = State::Complete;
                }
                Ok(again)
            }
            State::Complete => Ok(false),
        }
    }

    fn connect(&mut self, config: &ServerConfig) -> Result<bool, Error> {
        let State::Connecting {
            connector,
            addresses,
            last,
        } = &mut self.state
        else {
            return Err(Error::InvalidState);
        };
        if let Some(pending) = connector {
            if !std::mem::take(&mut self.target_ready) {
                return Ok(false);
            }
            match pending.advance() {
                Ok(false) => return Ok(false),
                Ok(true) => {
                    let target = pending.take_stream().ok_or(Error::InvalidState)?;
                    let prepared = target
                        .local_addr()
                        .map(Into::into)
                        .and_then(|bound| Prepared::new(target).map(|target| (target, bound)));
                    let (target, bound) = match prepared {
                        Ok(prepared) => prepared,
                        Err(error) => return self.fail(error.into(), config),
                    };
                    if let Err(error) = remaining(self.until, Instant::now()) {
                        return self.fail(error, config);
                    }
                    if let Err(error) = self.exchange()?.send_success(bound) {
                        return self.fail(error, config);
                    }
                    self.state = State::Reply {
                        target: Some(target),
                    };
                    self.until = deadline(
                        Instant::now(),
                        config.limits.handshake.min(Duration::from_secs(10)),
                    )?;
                    return Ok(true);
                }
                Err(error) => {
                    *last = Some(error);
                    *connector = None;
                    self.target_replaced = true;
                }
            }
        }
        while let Some(address) = addresses.pop_front() {
            match Connector::new(address, self.until) {
                Ok(pending) => {
                    *connector = Some(pending);
                    return Ok(false);
                }
                Err(error) => *last = Some(error),
            }
        }
        let error = last.take().unwrap_or(Error::InvalidEndpoint);
        self.fail(error, config)
    }

    pub(super) fn interests(&self) -> (Option<Interest>, Option<Interest>) {
        match &self.state {
            State::Exchange | State::Reply { .. } => {
                (self.exchange.as_ref().and_then(Exchange::interest), None)
            }
            State::Connecting { connector, .. } => {
                (None, connector.as_ref().map(|_| Interest::WRITABLE))
            }
            State::Relay(relay) => relay.interests(),
            _ => (None, None),
        }
    }

    pub(super) fn client(&mut self) -> Result<&mut TcpStream, Error> {
        if let State::Relay(relay) = &mut self.state {
            Ok(relay.client.get_mut())
        } else {
            self.exchange
                .as_mut()
                .ok_or(Error::InvalidState)?
                .transport_mut()
        }
    }

    pub(super) fn target(&mut self) -> Option<&mut TcpStream> {
        match &mut self.state {
            State::Connecting { connector, .. } => connector.as_mut()?.transport_mut().ok(),
            State::Reply { target } => target.as_mut().map(|prepared| &mut prepared.target),
            State::Relay(relay) => Some(&mut relay.target),
            _ => None,
        }
    }
}
