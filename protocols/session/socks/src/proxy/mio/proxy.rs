use super::{Shutdown, entry::Entry, resolver::Resolver};
use crate::{Server, error::Error, server::ServerConfig};
use mio::{
    Events, Interest, Poll, Registry, Token, Waker,
    net::{TcpListener, TcpStream},
};
use std::{
    collections::{HashMap, VecDeque},
    io,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const LISTENER: Token = Token(0);
const WAKE: Token = Token(1);
const BATCH: usize = 64;

/// A complete bounded Mio listening proxy with its own poll/token namespace.
///
/// CONNECT only. Two bounded resolver workers keep system DNS off the event loop;
/// at most 64 numeric answers per query are retained. Authentication and policy
/// callbacks still run on the loop and must be fast, nonblocking, and non-panicking.
/// DNS, authorization, and all dial attempts share one absolute connect deadline.
/// Replies have a separate budget of `min(handshake, 10s)`; relay timeout is total
/// lifetime, not idle time. Each relay allocates two 16 KiB directional buffers.
///
/// Dropping or stopping closes listener and sessions. In-flight system DNS cannot
/// be cancelled: at most two resolver threads may outlive the proxy until their
/// current calls return. Queued queries do not run after shutdown. This is not a
/// general executor; use `Client`/`Exchange`/`Connector` in an existing Mio loop.
pub struct Proxy {
    poll: Poll,
    events: Events,
    listener: Option<TcpListener>,
    local: SocketAddr,
    config: ServerConfig,
    shutdown: Shutdown,
    resolver: Resolver,
    sessions: HashMap<usize, Entry>,
    tokens: HashMap<Token, usize>,
    ready: VecDeque<usize>,
    next_token: usize,
    accepting: bool,
    listener_registered: bool,
    again: bool,
}

impl Proxy {
    /// Register an already bound Mio listener with explicit server policies.
    /// Construction does not accept connections or start a poll thread.
    ///
    /// # Errors
    /// Returns invalid limits, registration, resolver-thread, or allocation errors.
    pub fn new(mut listener: TcpListener, server: Server) -> Result<Self, Error> {
        server.config.limits.validate()?;
        server
            .config
            .limits
            .connections
            .checked_mul(2 * 16 * 1024)
            .filter(|bytes| isize::try_from(*bytes).is_ok())
            .ok_or(Error::InvalidLimits)?;
        let poll = Poll::new()?;
        let local = listener.local_addr()?;
        poll.registry()
            .register(&mut listener, LISTENER, Interest::READABLE)?;
        let waker = Arc::new(Waker::new(poll.registry(), WAKE)?);
        let shutdown = Shutdown {
            stopped: Arc::new(AtomicBool::new(false)),
            waker: waker.clone(),
        };
        let resolver = Resolver::new(server.config.limits.connections, &waker)?;
        Ok(Self {
            poll,
            events: Events::with_capacity(256),
            listener: Some(listener),
            local,
            config: server.config,
            shutdown,
            resolver,
            sessions: HashMap::new(),
            tokens: HashMap::new(),
            ready: VecDeque::new(),
            next_token: 2,
            accepting: true,
            listener_registered: true,
            again: false,
        })
    }

    /// Bound listening address, including an OS-assigned port.
    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    /// Obtain a cancellation handle before handing the proxy to a worker thread.
    pub fn shutdown_handle(&self) -> Shutdown {
        self.shutdown.clone()
    }

    /// Number of active handshake, resolution, dial, reply, and relay sessions.
    pub fn active_connections(&self) -> usize {
        self.sessions.len()
    }

    /// Run until shutdown. Session errors are reported and do not stop the listener.
    /// Callbacks must not block, panic, or expose credentials.
    ///
    /// # Errors
    /// Returns listener, poll, registration, or token-exhaustion errors.
    pub fn run(&mut self, mut report: impl FnMut(Error)) -> Result<(), Error> {
        while self.poll(None, &mut report)? {}
        Ok(())
    }

    /// Perform bounded work and at most one poll wait. `max_wait` can shorten,
    /// never extend, phase deadlines. Returns `false` after shutdown completes.
    /// Call again immediately on `true`; queued continuations force a zero wait.
    ///
    /// # Errors
    /// Returns a listener/poll/registration error; drop the proxy on failure.
    pub fn poll(
        &mut self,
        max_wait: Option<Duration>,
        mut report: impl FnMut(Error),
    ) -> Result<bool, Error> {
        if self.shutdown.stopped.load(Ordering::Acquire) {
            self.stop();
            return Ok(false);
        }
        self.again = false;
        self.answers();
        let now = Instant::now();
        for (&id, entry) in &mut self.sessions {
            if now >= entry.until && !entry.queued {
                entry.queued = true;
                self.ready.push_back(id);
            }
        }
        for _ in 0..BATCH {
            let Some(id) = self.ready.pop_front() else {
                break;
            };
            let Some(mut entry) = self.sessions.remove(&id) else {
                continue;
            };
            entry.queued = false;
            let result = entry.advance(&self.config, &self.resolver);
            match result {
                Ok(again) if !entry.complete() => {
                    self.register(id, &mut entry)?;
                    self.sessions.insert(id, entry);
                    if again {
                        self.enqueue(id, false);
                    }
                }
                Ok(_) => self.forget(id, &entry),
                Err(error) => {
                    self.forget(id, &entry);
                    report(error);
                }
            }
            if self.shutdown.stopped.load(Ordering::Acquire) {
                self.stop();
                return Ok(false);
            }
        }
        self.accept()?;
        let timeout = self.timeout(max_wait);
        match self.poll.poll(&mut self.events, timeout) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(true),
            Err(error) => return Err(error.into()),
        }
        for event in &self.events {
            let token = event.token();
            if token == LISTENER {
                self.accepting = true;
            } else if let Some(&id) = self.tokens.get(&token) {
                if let Some(entry) = self.sessions.get_mut(&id) {
                    // Any event for this fresh target is a hint to probe the socket.
                    // take_error/peer_addr, not readiness flags, confirm connect.
                    entry.target_ready |= entry.target_token == Some(token);
                    if !entry.queued {
                        entry.queued = true;
                        self.ready.push_back(id);
                    }
                }
            }
        }
        Ok(true)
    }

    fn stop(&mut self) {
        self.listener = None;
        self.sessions.clear();
        self.tokens.clear();
        self.ready.clear();
        self.resolver.stop();
    }

    fn token(&mut self) -> Result<Token, Error> {
        let token = Token(self.next_token);
        self.next_token = self.next_token.checked_add(1).ok_or(Error::InvalidLimits)?;
        Ok(token)
    }

    fn enqueue(&mut self, id: usize, target: bool) {
        if let Some(entry) = self.sessions.get_mut(&id) {
            entry.target_ready |= target;
            if !entry.queued {
                entry.queued = true;
                self.ready.push_back(id);
            }
        }
    }

    fn answers(&mut self) {
        for _ in 0..BATCH {
            let Some((id, result)) = self.resolver.answer() else {
                return;
            };
            if let Some(entry) = self.sessions.get_mut(&id) {
                if entry.accept_resolution(result) {
                    self.enqueue(id, false);
                }
            }
        }
        self.again = true;
    }

    fn accept(&mut self) -> Result<(), Error> {
        if self.sessions.len() == self.config.limits.connections {
            if self.listener_registered {
                if let Some(listener) = &mut self.listener {
                    self.poll.registry().deregister(listener)?;
                }
                self.listener_registered = false;
            }
            return Ok(());
        }
        if !self.listener_registered {
            if let Some(listener) = &mut self.listener {
                self.poll
                    .registry()
                    .register(listener, LISTENER, Interest::READABLE)?;
            }
            self.listener_registered = true;
            self.accepting = true;
        }
        if !self.accepting {
            return Ok(());
        }
        for _ in 0..BATCH {
            if self.sessions.len() == self.config.limits.connections {
                break;
            }
            let Some(listener) = self.listener.as_mut() else {
                break;
            };
            match listener.accept() {
                Ok((stream, peer)) => {
                    let token = self.token()?;
                    let id = token.0;
                    let entry = Entry::new(id, stream, peer, &self.config)?;
                    self.sessions.insert(id, entry);
                    self.enqueue(id, false);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    self.accepting = false;
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn timeout(&self, maximum: Option<Duration>) -> Option<Duration> {
        if self.again
            || !self.ready.is_empty()
            || (self.accepting && self.sessions.len() < self.config.limits.connections)
        {
            return Some(Duration::ZERO);
        }
        let now = Instant::now();
        self.sessions
            .values()
            .map(|entry| entry.until.saturating_duration_since(now))
            .chain(maximum)
            .min()
    }

    fn forget(&mut self, id: usize, entry: &Entry) {
        self.tokens.remove(&Token(id));
        if let Some(token) = entry.target_token {
            self.tokens.remove(&token);
        }
    }

    fn register(&mut self, id: usize, entry: &mut Entry) -> Result<(), Error> {
        let (client, target) = entry.interests();
        let previous = entry.client_interest;
        entry.client_interest = registration(
            self.poll.registry(),
            entry.client()?,
            previous,
            client,
            Token(id),
        )?;
        if client.is_some() {
            self.tokens.insert(Token(id), id);
        } else {
            self.tokens.remove(&Token(id));
        }
        if entry.target_replaced {
            if let Some(token) = entry.target_token.take() {
                self.tokens.remove(&token);
            }
            entry.target_interest = None;
            entry.target_replaced = false;
        }
        if target.is_some() && entry.target_token.is_none() {
            entry.target_token = Some(self.token()?);
        }
        if let Some(token) = entry.target_token {
            let previous = entry.target_interest;
            if let Some(stream) = entry.target() {
                entry.target_interest =
                    registration(self.poll.registry(), stream, previous, target, token)?;
            }
            if target.is_some() {
                self.tokens.insert(token, id);
            } else {
                self.tokens.remove(&token);
                entry.target_token = None;
            }
        }
        Ok(())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop();
    }
}

fn registration(
    registry: &Registry,
    stream: &mut TcpStream,
    old: Option<Interest>,
    new: Option<Interest>,
    token: Token,
) -> io::Result<Option<Interest>> {
    match (old, new) {
        (None, Some(interest)) => registry.register(stream, token, interest)?,
        (Some(_), None) => registry.deregister(stream)?,
        (Some(old), Some(new)) if old != new => registry.reregister(stream, token, new)?,
        _ => {}
    }
    Ok(new)
}
