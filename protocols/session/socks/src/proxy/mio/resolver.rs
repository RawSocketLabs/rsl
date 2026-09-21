use crate::{Destination, error::Error};
use mio::Waker;
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

// Bound both DNS request storage and address/result storage. Extra system answers
// are intentionally not used; policy sees each of the retained numeric candidates.
const MAX_ADDRESSES: usize = 64;
pub(super) type Answer = (usize, Result<Vec<SocketAddr>, Error>);
type Query = (usize, Destination);

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
                        let Ok((id, endpoint)) = query else {
                            break;
                        };
                        if stopped.load(Ordering::Acquire) {
                            break;
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

    pub(super) fn submit(&self, id: usize, endpoint: Destination) -> Result<(), Error> {
        let sender = self.requests.as_ref().ok_or(Error::InvalidState)?;
        sender.try_send((id, endpoint)).map_err(|error| {
            let kind = match error {
                mpsc::TrySendError::Full(_) => io::ErrorKind::WouldBlock,
                mpsc::TrySendError::Disconnected(_) => io::ErrorKind::BrokenPipe,
            };
            io::Error::new(kind, "SOCKS resolver unavailable").into()
        })
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
        Destination::Domain { name, port } => Ok((crate::io::domain_name(name)?, *port)
            .to_socket_addrs()?
            .take(MAX_ADDRESSES)
            .collect()),
        Destination::Ip { .. } => Ok(endpoint.socket_addr().into_iter().collect()),
    }
}
