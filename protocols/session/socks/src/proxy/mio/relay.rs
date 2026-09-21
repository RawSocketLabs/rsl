use crate::Stream;
use mio::{Interest, net::TcpStream};
use std::{
    io::{self, Read, Write},
    net::Shutdown,
};

const CAPACITY: usize = 16 * 1024;

struct Direction {
    bytes: Vec<u8>,
    start: usize,
    len: usize,
    eof: bool,
    closed: bool,
}

impl Direction {
    fn new() -> io::Result<Self> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(CAPACITY)
            .map_err(io::Error::other)?;
        bytes.resize(CAPACITY, 0);
        Ok(Self {
            bytes,
            start: 0,
            len: 0,
            eof: false,
            closed: false,
        })
    }

    fn readable(&self) -> bool {
        !self.eof && self.len < CAPACITY
    }
    fn writable(&self) -> bool {
        self.len != 0
    }

    fn step<R: Read, W: Write>(
        &mut self,
        source: &mut R,
        target: &mut W,
        budget: &mut usize,
    ) -> io::Result<bool> {
        let mut progress = false;
        if self.writable() && *budget != 0 {
            *budget -= 1;
            let end = (self.start + self.len).min(CAPACITY);
            match target.write(&self.bytes[self.start..end]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(count) => {
                    self.start = (self.start + count) % CAPACITY;
                    self.len -= count;
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => progress = true,
                Err(error) => return Err(error),
            }
        }
        if self.readable() && *budget != 0 {
            // Circular storage: a one-byte drain never moves the retained payload.
            let end = (self.start + self.len) % CAPACITY;
            let available = (CAPACITY - self.len).min(CAPACITY - end);
            *budget -= 1;
            match source.read(&mut self.bytes[end..end + available]) {
                Ok(0) => {
                    self.eof = true;
                    progress = true;
                }
                Ok(count) => {
                    self.len += count;
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => progress = true,
                Err(error) => return Err(error),
            }
        }
        Ok(progress)
    }

    fn shutdown(&mut self, target: &TcpStream) -> io::Result<()> {
        if self.eof && !self.writable() && !self.closed {
            target.shutdown(Shutdown::Write)?;
            self.closed = true;
        }
        Ok(())
    }
}

pub(super) struct Relay {
    pub(super) client: Stream<TcpStream>,
    pub(super) target: TcpStream,
    outbound: Direction,
    inbound: Direction,
}

// Reserve before writing success. The post-reply transition must not allocate.
pub(super) struct Prepared {
    pub(super) target: TcpStream,
    outbound: Direction,
    inbound: Direction,
}

impl Prepared {
    pub(super) fn new(target: TcpStream) -> io::Result<Self> {
        Ok(Self {
            target,
            outbound: Direction::new()?,
            inbound: Direction::new()?,
        })
    }

    pub(super) fn into_relay(self, client: Stream<TcpStream>) -> Relay {
        Relay {
            client,
            target: self.target,
            outbound: self.outbound,
            inbound: self.inbound,
        }
    }
}

impl Relay {
    pub(super) fn interests(&self) -> (Option<Interest>, Option<Interest>) {
        (
            interest(self.outbound.readable(), self.inbound.writable()),
            interest(self.inbound.readable(), self.outbound.writable()),
        )
    }

    pub(super) fn complete(&self) -> bool {
        self.outbound.closed && self.inbound.closed
    }

    // Budget exhaustion requires a continuation even without a new readiness edge.
    pub(super) fn advance(&mut self) -> io::Result<bool> {
        let mut budget = 64;
        while budget != 0 {
            let outgoing = self
                .outbound
                .step(&mut self.client, &mut self.target, &mut budget)?;
            let incoming = self
                .inbound
                .step(&mut self.target, &mut self.client, &mut budget)?;
            self.outbound.shutdown(&self.target)?;
            self.inbound.shutdown(self.client.get_ref())?;
            if !outgoing && !incoming {
                break;
            }
        }
        Ok(budget == 0)
    }
}

fn interest(read: bool, write: bool) -> Option<Interest> {
    match (read, write) {
        (true, true) => Some(Interest::READABLE | Interest::WRITABLE),
        (true, false) => Some(Interest::READABLE),
        (false, true) => Some(Interest::WRITABLE),
        (false, false) => None,
    }
}

#[cfg(test)]
mod unit {
    use super::Direction;
    use std::io::{self, Cursor, Read, Write};

    // Test at the Read/Write boundary, without inspecting queue indices/storage.
    struct Chunked<T> {
        inner: T,
        chunk: usize,
        calls: usize,
    }
    impl<T: Read> Read for Chunked<T> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            assert!(!bytes.is_empty() && bytes.len() <= 16 * 1024);
            let length = bytes.len().min(self.chunk);
            self.inner.read(&mut bytes[..length])
        }
    }
    impl<T: Write> Write for Chunked<T> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            assert!(!bytes.is_empty() && bytes.len() <= 16 * 1024);
            self.inner.write(&bytes[..bytes.len().min(self.chunk)])
        }
        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    #[test]
    fn tiny_writes_preserve_bytes_across_repeated_wraparound() {
        let expected: Vec<_> = (0..100_000)
            .map(|value| u8::try_from(value % 251).unwrap())
            .collect();
        for (read, write) in [(16384, 1), (7, 8191), (16383, 7), (1, 1)] {
            let mut source = Chunked {
                inner: Cursor::new(&expected),
                chunk: read,
                calls: 0,
            };
            let mut target = Chunked {
                inner: Vec::new(),
                chunk: write,
                calls: 0,
            };
            let mut direction = Direction::new().unwrap();
            // No blocking/errors in these fixtures: each byte needs at most one
            // read and one write, plus EOF. Lost progress must fail, not hang.
            for _ in 0..=(2 * expected.len() + 1).div_ceil(64) {
                if target.inner.len() == expected.len() {
                    break;
                }
                source.calls = 0;
                target.calls = 0;
                let mut budget = 64;
                while budget != 0
                    && direction
                        .step(&mut source, &mut target, &mut budget)
                        .unwrap()
                {}
                assert!(
                    source.calls + target.calls <= 64,
                    "read={read}, write={write}"
                );
            }
            assert_eq!(target.inner, expected, "read={read}, write={write}");
        }
    }
}
