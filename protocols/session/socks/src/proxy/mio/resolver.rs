// --- Standard library ---
use std::{
    io,
    net::{SocketAddr, ToSocketAddrs},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
};

// --- Workspace dependencies ---
use mio::Waker;

// --- Internal modules ---
use crate::{Destination, error::Error, server::resolve::domain_name};

// Bound both DNS request storage and address/result storage. Extra system answers
// are intentionally not used; policy sees each of the retained numeric candidates.
const MAX_ADDRESSES: usize = 64;
pub(super) type Answer = (usize, Result<Vec<SocketAddr>, Error>);
/// Session id, destination, and a flag the session sets once it no longer needs the answer.
type Query = (usize, Destination, Arc<AtomicBool>);

pub(super) struct Resolver {
    requests: Option<SyncSender<Query>>,
    answers: Option<Receiver<Answer>>,
    stopped: Arc<AtomicBool>,
}

impl Resolver {
    pub(super) fn new(capacity: usize, waker: &Arc<Waker>) -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<Query>(capacity);
        let (answers, receiver) = mpsc::sync_channel(capacity);
        let requests = Arc::new(Mutex::new(requests));
        let stopped = Arc::new(AtomicBool::new(false));
        let resolver = Self {
            requests: Some(sender),
            answers: Some(receiver),
            stopped: stopped.clone(),
        };
        for _ in 0..2 {
            let (requests, answers, stopped, waker) = (
                requests.clone(),
                answers.clone(),
                stopped.clone(),
                waker.clone(),
            );
            thread::Builder::new()
                .name("socks-dns".into())
                .spawn(move || {
                    loop {
                        let query = match requests.lock() {
                            Ok(receiver) => receiver.recv(),
                            Err(_) => break,
                        };
                        let Ok((id, endpoint, cancelled)) = query else {
                            break;
                        };
                        if stopped.load(Ordering::Acquire) {
                            break;
                        }
                        // Skip sessions that expired or closed while queued, so dead
                        // queries cannot hold workers away from live sessions.
                        if cancelled.load(Ordering::Acquire) {
                            continue;
                        }
                        let result = resolve(&endpoint);
                        if stopped.load(Ordering::Acquire) || answers.send((id, result)).is_err() {
                            break;
                        }
                        let _ = waker.wake();
                    }
                })?;
        }
        Ok(resolver)
    }

    /// Queue a lookup; the returned flag cancels it if set before a worker starts it.
    pub(super) fn submit(
        &self,
        id: usize,
        endpoint: Destination,
    ) -> Result<Arc<AtomicBool>, Error> {
        let sender = self.requests.as_ref().ok_or(Error::InvalidState)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        sender
            .try_send((id, endpoint, cancelled.clone()))
            .map_err(|error| {
                let kind = match error {
                    mpsc::TrySendError::Full(_) => io::ErrorKind::WouldBlock,
                    mpsc::TrySendError::Disconnected(_) => io::ErrorKind::BrokenPipe,
                };
                io::Error::new(kind, "SOCKS resolver unavailable").into()
            })
            .map(|()| cancelled)
    }

    pub(super) fn stop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.requests = None;
        self.answers = None; // Unblock workers whose bounded result channel is full.
    }

    pub(super) fn answer(&self) -> Option<Answer> {
        self.answers.as_ref()?.try_recv().ok()
    }
}

impl Drop for Resolver {
    fn drop(&mut self) {
        self.stop();
    }
}

fn resolve(endpoint: &Destination) -> Result<Vec<SocketAddr>, Error> {
    match endpoint {
        Destination::Domain { name, port } => Ok((domain_name(name)?, *port)
            .to_socket_addrs()?
            .take(MAX_ADDRESSES)
            .collect()),
        Destination::Ip { .. } => Ok(endpoint.socket_addr().into_iter().collect()),
    }
}

#[cfg(test)]
mod unit {
    // --- Standard library ---
    use std::{
        net::{Ipv4Addr, SocketAddr},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    // --- Workspace dependencies ---
    use mio::{Poll, Token, Waker};

    // --- Internal modules ---
    use super::Resolver;
    use crate::Destination;

    /// Wait for one answer from the workers, failing after a generous budget.
    fn next_answer(resolver: &Resolver) -> usize {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some((id, _)) = resolver.answer() {
                return id;
            }
            assert!(Instant::now() < until, "resolver produced no answer");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn cancelled_queries_are_skipped_without_an_answer() {
        let poll = Poll::new().unwrap();
        let waker = Arc::new(Waker::new(poll.registry(), Token(0)).unwrap());
        let resolver = Resolver::new(4, &waker).unwrap();
        let target = Destination::from(SocketAddr::from((Ipv4Addr::LOCALHOST, 9)));

        // Queue an already-cancelled query ahead of a live one.
        resolver
            .requests
            .as_ref()
            .unwrap()
            .try_send((1, target.clone(), Arc::new(AtomicBool::new(true))))
            .unwrap();
        let live = resolver.submit(2, target).unwrap();
        assert!(!live.load(Ordering::Acquire));

        assert_eq!(next_answer(&resolver), 2);
        // The cancelled query was dequeued first, so no later answer can appear for it.
        thread::sleep(Duration::from_millis(50));
        assert!(resolver.answer().is_none());
    }
}
