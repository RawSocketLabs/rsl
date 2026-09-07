#![no_main]

use libfuzzer_sys::fuzz_target;
use socks::blocking;
use socks::session::{ClientAuth, ServerAuth};
use socks::v5::Endpoint;
use std::io::{self, Cursor, Read, Write};
use std::net::SocketAddr;

// Input and output are separate so server replies cannot overwrite hostile input.
struct Stream<'a>(Cursor<&'a [u8]>);

impl Read for Stream<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
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
            let mut stream = Stream(Cursor::new(input.as_slice()));
            if let Ok(incoming) = blocking::accept(&mut stream, &auth) {
                let _ = incoming.accept(destination());
            }
            // Largest greeting + credentials + CONNECT bound consumed input.
            assert!(stream.0.position() <= 257 + 513 + 262);
        }
    }
    for auth in [
        ClientAuth::NoAuthentication,
        ClientAuth::UsernamePassword {
            username: b"u",
            password: b"p",
        },
    ] {
        let mut stream = Stream(Cursor::new(bytes));
        let _ = blocking::connect(&mut stream, destination(), auth);
        assert!(stream.0.position() <= 2 + 2 + 262);
    }
});
