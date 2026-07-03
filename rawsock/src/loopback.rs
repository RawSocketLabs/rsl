//! An in-memory [`RawIo`] backend for tests and dry runs — no privilege, any OS.
//!
//! `send_raw` records the bytes; `recv` returns them in order (so a test can send then
//! read back). [`Loopback::sent`] exposes everything transmitted for direct assertions.
//! Because the bulk of `rawsock`'s logic is byte-shaping, this backend carries most of the
//! unit coverage with no `CAP_NET_RAW`.

use std::collections::VecDeque;
use std::io;

use crate::{Layer, RawIo};

/// An in-memory sink + source implementing [`RawIo`].
#[derive(Debug, Clone)]
pub struct Loopback {
    layer: Layer,
    queue: VecDeque<Vec<u8>>,
    log: Vec<Vec<u8>>,
}

impl Loopback {
    /// A loopback handle reporting `layer`.
    #[must_use]
    pub fn new(layer: Layer) -> Self {
        Self {
            layer,
            queue: VecDeque::new(),
            log: Vec::new(),
        }
    }

    /// Every byte string transmitted through `send_raw`, in order.
    #[must_use]
    pub fn sent(&self) -> &[Vec<u8>] {
        &self.log
    }

    /// The most recently transmitted byte string, if any.
    #[must_use]
    pub fn last_sent(&self) -> Option<&[u8]> {
        self.log.last().map(Vec::as_slice)
    }
}

impl RawIo for Loopback {
    fn send_raw(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.queue.push_back(bytes.to_vec());
        self.log.push(bytes.to_vec());
        Ok(bytes.len())
    }

    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.queue.pop_front() {
            Some(frame) => {
                let n = frame.len().min(buf.len());
                buf[..n].copy_from_slice(&frame[..n]);
                Ok(n)
            }
            None => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "loopback queue empty",
            )),
        }
    }

    fn layer(&self) -> Layer {
        self.layer
    }
}
