//! `cargo run -p socks --features mio --example mio_client -- 127.0.0.1:1080 example.com 80`
//! Native Mio setup with explicit partial-write accounting. The same Poll lives
//! through TCP connect, SOCKS negotiation, and application reads/writes.
use mio::{Events, Interest, Poll, Token};
use socks::{io::mio::Connector, v5::Endpoint, v5::auth::ClientAuth, v5::client::mio::Client};
use std::{
    io::{self, Read, Write},
    net::Shutdown,
    time::{Duration, Instant},
};

fn wait(poll: &mut Poll, events: &mut Events, until: Instant) -> io::Result<()> {
    let timeout = until
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or(io::ErrorKind::TimedOut)?;
    match poll.poll(events, Some(timeout)) {
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(()),
        result => result,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let proxy = args
        .next()
        .ok_or("supply proxy address, destination, and port")?
        .parse()?;
    let host = args.next().ok_or("supply destination host")?;
    let port = args.next().ok_or("supply destination port")?.parse()?;
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(16);
    let until = Instant::now() + Duration::from_secs(10);
    let mut connector = Connector::new(proxy, until)?;
    poll.registry()
        .register(connector.transport_mut()?, Token(0), Interest::WRITABLE)?;
    loop {
        wait(&mut poll, &mut events, until)?;
        if connector.advance()? {
            break;
        }
    }
    let socket = connector.take_stream().ok_or("connect did not complete")?;
    let mut client = Client::new(
        socket,
        Endpoint::domain(host.as_bytes(), port)?,
        ClientAuth::NoAuthentication,
    )?;
    while !client.advance()? {
        let interest = client.interest().ok_or("missing handshake interest")?;
        poll.registry()
            .reregister(client.transport_mut()?, Token(0), interest)?;
        if !client.needs_advance() {
            wait(&mut poll, &mut events, until)?;
        }
        if Instant::now() >= until {
            return Err(io::Error::from(io::ErrorKind::TimedOut).into());
        }
    }
    let (mut stream, _bound) = client.take_stream().ok_or("handshake did not complete")?;
    // A fixed HTTP/1.0 probe avoids interpolating untrusted command-line text into
    // HTTP headers; the destination is still forwarded unchanged to the proxy.
    let request = b"GET / HTTP/1.0\r\n\r\n";
    let mut sent = 0;
    poll.registry()
        .reregister(stream.get_mut(), Token(0), Interest::WRITABLE)?;
    while sent != request.len() {
        match stream.write(&request[sent..]) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
            Ok(count) => sent += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                wait(&mut poll, &mut events, until)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    stream.get_ref().shutdown(Shutdown::Write)?;
    poll.registry()
        .reregister(stream.get_mut(), Token(0), Interest::READABLE)?;
    let mut bytes = [0; 8192];
    loop {
        if Instant::now() >= until {
            return Err(io::Error::from(io::ErrorKind::TimedOut).into());
        }
        match stream.read(&mut bytes) {
            Ok(0) => break,
            Ok(count) => io::stdout().write_all(&bytes[..count])?,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                wait(&mut poll, &mut events, until)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
