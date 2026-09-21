//! Convenience-owned Mio poll loop.
use super::Client;
use crate::{
    Stream,
    error::Error,
    io::mio::{Connector, deadline, remaining},
    v5::{Endpoint, auth::ClientAuth},
};
use mio::{Events, Interest, Poll, Token, net::TcpStream};
use std::{
    io,
    net::SocketAddr,
    time::{Duration, Instant},
};

/// Connect and negotiate using a private Mio poll loop and one absolute timeout.
/// Returns the original poll, nonblocking stream, and proxy's bound endpoint.
/// The socket is deregistered: register it with the **returned poll**, which must
/// remain alive while using the socket. Mio sources are associated with their
/// original poll for their lifetime; transferring to another poll is not portable.
/// Use `Connector` plus `Client` to integrate with an existing event loop instead.
///
/// # Errors
/// Returns local validation, connection, handshake, or deadline errors.
pub fn connect_tcp(
    proxy: SocketAddr,
    destination: Endpoint,
    auth: ClientAuth<'_>,
    timeout: Duration,
) -> Result<(Poll, Stream<TcpStream>, Endpoint), Error> {
    destination.validate_connect_destination()?;
    let credentials = auth.credentials()?;
    let until = deadline(Instant::now(), timeout)?;
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(8);
    let mut connector = Connector::new(proxy, until)?;
    poll.registry()
        .register(connector.transport_mut()?, Token(0), Interest::WRITABLE)?;
    loop {
        wait(&mut poll, &mut events, until)?;
        if connector.advance()? {
            break;
        }
    }
    let mut socket = connector.take_stream().ok_or(Error::InvalidState)?;
    poll.registry().deregister(&mut socket)?;
    let mut client = Client::prepared(socket, destination, auth.method(), credentials)?;
    let mut registered = false;
    loop {
        remaining(until, Instant::now())?;
        let established = client.advance()?;
        remaining(until, Instant::now())?;
        if established {
            break;
        }
        let interest = client.interest().ok_or(Error::InvalidState)?;
        if registered {
            poll.registry()
                .reregister(client.transport_mut()?, Token(0), interest)?;
        } else {
            poll.registry()
                .register(client.transport_mut()?, Token(0), interest)?;
            registered = true;
        }
        if !client.needs_advance() {
            wait(&mut poll, &mut events, until)?;
        }
    }
    if registered {
        poll.registry().deregister(client.transport_mut()?)?;
    }
    let (stream, bound) = client.take_stream().ok_or(Error::InvalidState)?;
    Ok((poll, stream, bound))
}

fn wait(poll: &mut Poll, events: &mut Events, until: Instant) -> Result<(), Error> {
    match poll.poll(events, Some(remaining(until, Instant::now())?)) {
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(()),
        result => Ok(result?),
    }
}
