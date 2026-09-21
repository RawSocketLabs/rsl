#![no_main]
use libfuzzer_sys::fuzz_target;
use socks::{
    server::policy::ServerAuth,
    v5::Endpoint,
    v5::auth::ClientAuth,
    v5::{
        client::mio::Client,
        server::mio::{Exchange, Stage},
    },
};
use std::{
    io::{self, Cursor, Read, Write},
    net::SocketAddr,
};

struct Fragmented<'a> {
    input: Cursor<&'a [u8]>,
    chunk: usize,
    operations: usize,
}
impl Fragmented<'_> {
    fn step(&mut self) -> io::Result<()> {
        self.operations += 1;
        match self.operations % 3 {
            0 => Ok(()),
            1 => Err(io::ErrorKind::WouldBlock.into()),
            _ => Err(io::ErrorKind::Interrupted.into()),
        }
    }
}
impl Read for Fragmented<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.step()?;
        let count = bytes.len().min(self.chunk);
        self.input.read(&mut bytes[..count])
    }
}
impl Write for Fragmented<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.step()?;
        Ok(bytes.len().min(self.chunk))
    }
    fn flush(&mut self) -> io::Result<()> {
        self.step()
    }
}
fn target() -> Endpoint {
    SocketAddr::from(([127, 0, 0, 1], 80)).into()
}
fn raw<'a>(bytes: &'a [u8], chunk: usize) -> Fragmented<'a> {
    Fragmented {
        input: Cursor::new(bytes),
        chunk,
        operations: 0,
    }
}
fn stage(server: &mut Exchange<Fragmented<'_>>) -> Result<Stage, socks::error::Error> {
    for _ in 0..8192 {
        match server.advance()? {
            Stage::Exchanging => {}
            stage => return Ok(stage),
        }
    }
    panic!("bounded handshake must make progress");
}
fn complete(client: &mut Client<Fragmented<'_>>) -> Result<(), socks::error::Error> {
    for _ in 0..8192 {
        if client.advance()? {
            return Ok(());
        }
    }
    panic!("bounded handshake must make progress");
}
fn payload(mut stream: socks::Stream<Fragmented<'_>>, expected: &[u8]) {
    let mut received = Vec::new();
    let mut bytes = [0; 513];
    loop {
        match stream.read(&mut bytes) {
            Ok(0) => break,
            Ok(count) => {
                received.extend_from_slice(&bytes[..count]);
                assert!(
                    received.len() <= expected.len(),
                    "handoff cannot duplicate payload"
                );
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => panic!("{error}"),
        }
    }
    assert_eq!(received, expected);
}
fuzz_target!(|bytes: &[u8]| {
    let chunk = [1, 2, 7, 513][usize::from(bytes.first().copied().unwrap_or(0)) % 4];
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
            let mut server = Exchange::new(raw(&input, chunk), auth);
            if let Ok(Stage::Requested) = stage(&mut server) {
                server.send_success(target()).unwrap();
                assert_eq!(stage(&mut server).unwrap(), Stage::Established);
            }
        }
    }
    let auth = ClientAuth::UsernamePassword {
        username: b"u",
        password: b"p",
    };
    for auth in [ClientAuth::NoAuthentication, auth] {
        let mut client = Client::new(raw(bytes, chunk), target(), auth).unwrap();
        let _ = complete(&mut client);
    }
    // Independent RFC transcripts plus arbitrary tails prove byte-exact handoff.
    let input = [
        b"\x05\x02\x01\x00\x05\x00\x00\x01\x7f\x00\x00\x01\x00\x50".as_slice(),
        bytes,
    ]
    .concat();
    let mut client = Client::new(raw(&input, chunk), target(), auth).unwrap();
    complete(&mut client).unwrap();
    payload(client.take_stream().unwrap().0, bytes);
    let input = [
        b"\x05\x01\x02\x01\x01u\x01p\x05\x01\x00\x01\x7f\x00\x00\x01\x00\x50".as_slice(),
        bytes,
    ]
    .concat();
    let mut server = Exchange::new(
        raw(&input, chunk),
        ServerAuth::username_password(|u, p| u == b"u" && p == b"p"),
    );
    assert_eq!(stage(&mut server).unwrap(), Stage::Requested);
    server.send_success(target()).unwrap();
    assert_eq!(stage(&mut server).unwrap(), Stage::Established);
    payload(server.take_stream().unwrap(), bytes);
});
