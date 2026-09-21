use std::io::{self, Read, Write};

// Independent RFC 1928 fixtures: 127.0.0.1:1080, CONNECT and success reply.
pub(crate) const REQUEST: &[u8] = &[5, 1, 0, 1, 127, 0, 0, 1, 4, 56];
pub(crate) const REPLY: &[u8] = &[5, 0, 0, 1, 127, 0, 0, 1, 4, 56];

// The opaque domain also proves that zero-port rejection precedes system resolution.
pub(crate) fn zero_port_requests() -> [Vec<u8>; 3] {
    [
        vec![5, 1, 0, 1, 127, 0, 0, 1, 0, 0],
        vec![5, 1, 0, 3, 1, 0xff, 0, 0],
        vec![
            5, 1, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0,
        ],
    ]
}

// RFC 1929 §2 maximum credentials, then an RFC 1928 CONNECT request.
pub(crate) fn maximum_credentials_transcript() -> Vec<u8> {
    let mut input = vec![5, 1, 2, 1, 255];
    input.extend_from_slice(&[7; 255]);
    input.push(255);
    input.extend_from_slice(&[8; 255]);
    input.extend_from_slice(REQUEST);
    input
}

// RFC 1928 §4: domain octet count excludes the port; IPv6 is exactly 16 octets.
pub(crate) fn variable_address_vectors() -> Vec<(socks::v5::Endpoint, Vec<u8>)> {
    let domain = vec![b'x'; 255];
    let mut domain_wire = vec![5, 1, 0, 3, 255];
    domain_wire.extend_from_slice(&domain);
    domain_wire.extend_from_slice(&[1, 187]); // port 443, network byte order
    vec![
        (
            socks::v5::Endpoint::domain(domain, 443).unwrap(),
            domain_wire,
        ),
        (
            socks::v5::Endpoint::Ipv6 {
                address: std::net::Ipv6Addr::LOCALHOST,
                port: 443,
            },
            vec![
                5, 1, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 187,
            ],
        ),
    ]
}

pub(crate) struct Script {
    pub incoming: io::Cursor<Vec<u8>>,
    pub written: Vec<u8>,
    pub chunk: usize,
    pub read_calls: usize,
    pub max_read_len: usize,
    pub read_error: Option<io::ErrorKind>,
    pub read_error_at: Option<(u64, io::ErrorKind)>,
    pub flush_error: Option<io::ErrorKind>,
    #[cfg(feature = "tokio")]
    pub read_pending: bool,
}

impl Script {
    pub(crate) fn new(incoming: impl Into<Vec<u8>>, chunk: usize) -> Self {
        Self {
            incoming: io::Cursor::new(incoming.into()),
            written: Vec::new(),
            chunk,
            read_calls: 0,
            max_read_len: 0,
            read_error: None,
            read_error_at: None,
            flush_error: None,
            #[cfg(feature = "tokio")]
            read_pending: false,
        }
    }
}

impl Read for Script {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.read_calls += 1;
        self.max_read_len = self.max_read_len.max(bytes.len());
        if let Some(error) = self.read_error.take() {
            return Err(io::Error::new(error, "scripted transport failure"));
        }
        if let Some((offset, error)) = self.read_error_at {
            if self.incoming.position() >= offset {
                self.read_error_at = None;
                return Err(io::Error::new(error, "scripted partial-read failure"));
            }
        }
        let len = bytes.len().min(self.chunk);
        self.incoming.read(&mut bytes[..len])
    }
}

impl Write for Script {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let len = bytes.len().min(self.chunk);
        self.written.extend_from_slice(&bytes[..len]);
        Ok(len)
    }
    fn flush(&mut self) -> io::Result<()> {
        match self.flush_error.take() {
            Some(error) => Err(io::Error::from(error)),
            None => Ok(()),
        }
    }
}

#[cfg(feature = "tokio")]
impl tokio::io::AsyncRead for Script {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        if self.read_pending {
            return std::task::Poll::Pending;
        }
        let bytes = buffer.initialize_unfilled();
        let read = Read::read(&mut *self, bytes)?;
        buffer.advance(read);
        std::task::Poll::Ready(Ok(()))
    }
}

#[cfg(feature = "tokio")]
impl tokio::io::AsyncWrite for Script {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::task::Poll::Ready(Write::write(&mut *self, bytes))
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Write::flush(&mut *self))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
