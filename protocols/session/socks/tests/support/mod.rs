use std::io::{self, Read, Write};

// Independent RFC 1928 fixtures: 127.0.0.1:1080, CONNECT and success reply.
pub(crate) const REQUEST: &[u8] = &[5, 1, 0, 1, 127, 0, 0, 1, 4, 56];
pub(crate) const REPLY: &[u8] = &[5, 0, 0, 1, 127, 0, 0, 1, 4, 56];

// RFC 1928 §4: domain octet count excludes the port; IPv6 is exactly 16 octets.
pub(crate) fn variable_address_vectors() -> Vec<(socks::v5::Endpoint, Vec<u8>)> {
    let domain = vec![b'x'; 255];
    let mut domain_wire = vec![5, 1, 0, 3, 255];
    domain_wire.extend_from_slice(&domain);
    domain_wire.extend_from_slice(&[1, 187]); // port 443, network byte order
    vec![
        (socks::v5::Endpoint::domain(domain, 443), domain_wire),
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
}

impl Script {
    pub(crate) fn new(incoming: impl Into<Vec<u8>>, chunk: usize) -> Self {
        Self {
            incoming: io::Cursor::new(incoming.into()),
            written: Vec::new(),
            chunk,
        }
    }
}

impl Read for Script {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
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
        Ok(())
    }
}

#[cfg(feature = "tokio")]
impl tokio::io::AsyncRead for Script {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
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
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
