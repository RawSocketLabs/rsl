//! The blocking stream.

// --- Standard library ---
use std::io::{self, Read, Write};

// --- Workspace dependencies ---
use crypto::RandomSource;

// --- Internal modules ---
use super::Error;
use crate::{Client, ClientConfig, client};

/// A TLS connection over a blocking transport.
///
/// [`connect`](Self::connect) runs the handshake; after it the stream is `Read` and `Write`
/// for application data. Reading returns `Ok(0)` once the peer has sent `close_notify`, and
/// fails with `UnexpectedEof` if the transport ends without one, because data may then be
/// missing. Each write is handed to the transport before it returns; `flush` also flushes the
/// transport.
///
/// A write reports how many bytes it accepted. If the transport fails partway through,
/// the stream keeps the unsent remainder. It retries on the next write, flush or close,
/// and before a read that needs more input from the transport. A persistent transport
/// error is returned to the caller.
///
/// The transport must block: a `WouldBlock` from it is returned as an error. The random
/// source is the caller's, as everywhere in this crate: pass `os_random::OsRandom` outside
/// tests.
pub struct Stream<'a, T> {
    client: Client<'a>,
    transport: T,
    /// Bytes read from the transport that the client has not taken yet.
    pending: Vec<u8>,
}

impl<'a, T: Read + Write> Stream<'a, T> {
    /// How many bytes one transport read asks for: a little more than one full record.
    const READ_SIZE: usize = 18 * 1024;

    /// Performs the TLS handshake over `transport` and returns the established stream.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the transport fails, and [`Error::Tls`] when the handshake does:
    /// the alert that tells the server why has been sent by then, if one was owed. A
    /// transport that ends mid-handshake is [`client::Error::Truncated`](crate::Error).
    pub fn connect(
        transport: T,
        config: ClientConfig<'a>,
        random: &mut impl RandomSource,
    ) -> Result<Self, Error> {
        let mut stream = Self {
            client: Client::new(config, random)?,
            transport,
            pending: Vec::new(),
        };
        while !stream.client.is_established() {
            stream.send()?;
            stream.receive()?;
        }
        Ok(stream)
    }

    /// The connection's state: the server's certificate chain, its session id, and whether
    /// the peer has closed.
    #[must_use]
    pub const fn client(&self) -> &Client<'a> {
        &self.client
    }

    /// The transport underneath.
    #[must_use]
    pub const fn get_ref(&self) -> &T {
        &self.transport
    }

    /// Sends `close_notify`: this end will write nothing more. Reading still works until the
    /// peer closes too. A peer that answers by ending the transport without a `close_notify`
    /// of its own, as some servers do, makes the next read fail with `UnexpectedEof`. Calling
    /// this again does nothing.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the alert cannot be sent, and [`Error::Tls`] when the connection
    /// has already failed.
    pub fn close(&mut self) -> Result<(), Error> {
        self.client.close()?;
        self.send_and_flush()
    }

    /// Writes everything the client has queued to the transport.
    fn send(&mut self) -> Result<(), Error> {
        while !self.client.transmit().is_empty() {
            let count = match self.transport.write(self.client.transmit()) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
                Ok(count) => count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            };
            self.client.sent(count);
        }
        Ok(())
    }

    /// Deliver queued records through both the TLS and transport buffers.
    fn send_and_flush(&mut self) -> Result<(), Error> {
        self.send()?;
        Ok(self.transport.flush()?)
    }

    /// Reads from the transport if nothing is pending, then gives the client what it will
    /// take. Whatever the client queues in response, an alert included, is sent.
    ///
    /// Callers only ask for input when the client has no plaintext waiting and the peer has
    /// not closed, so a client that wants none here is one whose connection is over.
    fn receive(&mut self) -> Result<(), Error> {
        if !self.client.wants_receive() {
            return Err(client::Error::InvalidState.into());
        }
        if self.pending.is_empty() {
            // The peer cannot answer what a buffering transport has not sent yet.
            self.send_and_flush()?;
            self.read_transport()?;
        }
        while !self.pending.is_empty() && self.client.wants_receive() {
            match self.client.receive(&self.pending) {
                Ok(0) => break,
                Ok(taken) => drop(self.pending.drain(..taken)),
                Err(error) => {
                    // Best effort: the peer is owed the alert even though we are failing.
                    self.send_and_flush().ok();
                    return Err(error.into());
                }
            }
        }
        if self.client.peer_closed() {
            // The peer may already be gone. Its authenticated close still means clean EOF.
            self.send_and_flush().ok();
            Ok(())
        } else {
            self.send()
        }
    }

    /// Reads once from the transport into the pending bytes, or tells the client the
    /// transport has ended.
    fn read_transport(&mut self) -> Result<(), Error> {
        let start = self.pending.len();
        self.pending.resize(start + Self::READ_SIZE, 0);
        let result = loop {
            match self.transport.read(&mut self.pending[start..]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                result => break result,
            }
        };
        self.pending
            .truncate(start + *result.as_ref().unwrap_or(&0));
        if result? == 0 {
            self.client.receive_eof()?;
        }
        Ok(())
    }
}

impl<T: Read + Write> Read for Stream<'_, T> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            let plaintext = self.client.plaintext();
            if !plaintext.is_empty() {
                let count = plaintext.len().min(buffer.len());
                buffer[..count].copy_from_slice(&plaintext[..count]);
                self.client.consume(count);
                return Ok(count);
            }
            if self.client.peer_closed() {
                return Ok(0);
            }
            self.receive()?;
        }
    }
}

impl<T: Read + Write> Write for Stream<'_, T> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        // First whatever an earlier call left unsent; its error, if any, surfaces here.
        self.send()?;
        let accepted = self.client.write(buffer).map_err(Error::Tls)?;
        match self.send() {
            // The bytes are queued, so they count as written. The next call sends the rest,
            // or reports the transport's error if it is still failing.
            Err(_) if accepted > 0 => Ok(accepted),
            Err(error) => Err(error.into()),
            Ok(()) => Ok(accepted),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.send_and_flush().map_err(Into::into)
    }
}
