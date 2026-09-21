#![no_main]

use libfuzzer_sys::fuzz_target;
use socks::v5::Endpoint;
use socks::v5::{client::blocking as client, server::blocking as server};
use socks::{server::policy::ServerAuth, v5::auth::ClientAuth};
use std::io::{self, Cursor, Read, Write};
use std::net::SocketAddr;

// Input and output are separate so server replies cannot overwrite hostile input.
struct Stream<'a> {
    input: Cursor<&'a [u8]>,
    chunk: usize,
    max_read: usize,
}

impl<'a> Stream<'a> {
    fn new(bytes: &'a [u8], chunk: usize) -> Self {
        Self {
            input: Cursor::new(bytes),
            chunk,
            max_read: 0,
        }
    }
}

impl Read for Stream<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.max_read = self.max_read.max(bytes.len());
        let count = bytes.len().min(self.chunk);
        self.input.read(&mut bytes[..count])
    }
}

impl Write for Stream<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fuzz_target!(|bytes: &[u8]| {
    let destination = || Endpoint::from(SocketAddr::from(([127, 0, 0, 1], 80)));
    let chunk = [1, 2, 7, 513, usize::MAX][usize::from(bytes.first().copied().unwrap_or(0)) % 5];
    // Also enter CONNECT directly after valid no-auth/auth handshakes, so deep
    // framing is exercised without first rediscovering both negotiation prefixes.
    for prefix in [
        b"".as_slice(),
        b"\x05\x01\x00",
        b"\x05\x01\x02\x01\x01u\x01p",
    ] {
        let input = [prefix, bytes].concat();
        for auth in [
            ServerAuth::no_authentication(),
            ServerAuth::username_password(|_, _| true),
        ] {
            let mut stream = Stream::new(&input, chunk);
            if let Ok(incoming) = server::exchange(&mut stream, &auth) {
                let _ = incoming.send_success(destination());
            }
            assert!(
                stream.max_read <= 513,
                "never request more than the receive bound"
            );
        }
    }
    for auth in [
        ClientAuth::NoAuthentication,
        ClientAuth::UsernamePassword {
            username: b"u",
            password: b"p",
        },
    ] {
        let mut stream = Stream::new(bytes, chunk);
        let _ = client::connect(&mut stream, destination(), auth);
        assert!(stream.max_read <= 513);
    }

    // RFC-derived complete transcripts followed by arbitrary application bytes.
    // This oracle is independent of frame-length calculations and must hold at
    // every read partition, including tails larger than the handshake buffer.
    let request = b"\x05\x01\x00\x01\x7f\x00\x00\x01\x00\x50";
    let reply = b"\x05\x00\x00\x01\x7f\x00\x00\x01\x00\x50";
    let input = [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), request, bytes].concat();
    let policy = ServerAuth::username_password(|u, p| u == b"u" && p == b"p");
    let tunnel = server::exchange(Stream::new(&input, chunk), &policy)
        .unwrap()
        .send_success(destination())
        .unwrap();
    assert!(tunnel.get_ref().max_read <= 513);
    let (stream, buffered) = tunnel.into_parts();
    let mut tunnel = socks::Stream::from_parts(stream, buffered);
    let mut payload = Vec::new();
    tunnel.read_to_end(&mut payload).unwrap();
    assert_eq!(payload, bytes);
    assert!(tunnel.try_into_inner().is_ok());

    let input = [b"\x05\x02\x01\x00".as_slice(), reply, bytes].concat();
    let (mut tunnel, _) = client::connect(
        Stream::new(&input, chunk),
        destination(),
        ClientAuth::UsernamePassword {
            username: b"u",
            password: b"p",
        },
    )
    .unwrap();
    assert!(tunnel.get_ref().max_read <= 513);
    payload.clear();
    tunnel.read_to_end(&mut payload).unwrap();
    assert_eq!(payload, bytes);
    assert!(tunnel.try_into_inner().is_ok());
});
