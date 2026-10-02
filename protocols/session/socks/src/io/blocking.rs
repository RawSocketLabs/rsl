// --- Standard library ---
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};

// --- Internal modules ---
use crate::error::Error;
pub(crate) fn deadline(timeout: Duration) -> Result<Instant, Error> {
    super::deadline::deadline(Instant::now(), timeout)
}

pub(crate) fn remaining(deadline: Instant) -> io::Result<Duration> {
    super::deadline::remaining(deadline, Instant::now())
}

pub(crate) struct Deadline {
    pub(crate) stream: TcpStream,
    pub(crate) deadline: Instant,
}

impl Read for Deadline {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.stream
            .set_read_timeout(Some(remaining(self.deadline)?))?;
        self.stream.read(bytes)
    }
}

impl Write for Deadline {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.stream
            .set_write_timeout(Some(remaining(self.deadline)?))?;
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
