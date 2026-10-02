// --- Internal modules ---
use crate::{Destination, Stream};

/// One established connection through a SOCKS proxy, including its bound address.
/// Application I/O preserves the stream's prefetched prefix. This is distinct from
/// the server's two-sided relay connection and does not own a Mio poll: callers
/// must keep the original poll alive and reuse it for the transport.
/// No `Debug` implementation is provided because retained storage can contain credentials.
///
/// ```no_run
/// # #[cfg(feature = "blocking")]
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use socks::{Client, Destination, v5::client::Config};
/// use std::io::{Read, Write};
///
/// // Establish a connection through the proxy before sending application data.
/// let client = Client::configure(Config::no_authentication()).blocking().build()?;
/// let mut connection = client.connect(
///     "127.0.0.1:1080".parse()?, Destination::domain(b"example.com", 80),
/// )?;
///
/// // Ordinary I/O keeps any bytes prefetched during negotiation.
/// connection.write_all(b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n")?;
/// let mut response = Vec::new();
/// connection.read_to_end(&mut response)?;
///
/// // Transfer ownership without discarding unread buffered data.
/// let (stream, bound) = connection.into_parts();
/// # Ok(()) }
/// ```
#[must_use = "use the connection or transfer its buffered stream with into_parts"]
pub struct Connection<S> {
    pub(super) stream: Stream<S>,
    bound: Destination,
}

impl<S> Connection<S> {
    /// Pair a successfully negotiated stream with the proxy's reported bound address.
    pub(in crate::client) fn new(stream: Stream<S>, bound: Destination) -> Self {
        Self { stream, bound }
    }

    /// The proxy's bound address, not the requested destination or the proxy's listening address.
    #[must_use]
    pub fn bound(&self) -> &Destination {
        &self.bound
    }

    /// Inspect or configure the transport; direct reads bypass buffered data.
    #[must_use]
    pub fn get_ref(&self) -> &S {
        self.stream.get_ref()
    }

    /// Configure or register the transport; perform application I/O through this connection.
    /// Direct transport reads bypass retained bytes and can reorder input.
    pub fn get_mut(&mut self) -> &mut S {
        self.stream.get_mut()
    }

    /// Transfer both the buffered stream and bound address without discarding unread bytes.
    pub fn into_parts(self) -> (Stream<S>, Destination) {
        (self.stream, self.bound)
    }
}
