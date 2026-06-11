//! Smoke test: the fastest possible "is it alive" check — a server binds, a
//! client negotiates through it, and a single byte makes the round trip. If
//! this fails, nothing else is worth running.

mod common;

use std::io::{Read, Write};

use socks::client::Client;
use socks::server::Server;

use common::{spawn_proxy, spawn_tcp_echo};

#[test]
fn proxy_relays_a_single_byte() {
    let echo = spawn_tcp_echo();
    let (proxy, server) = spawn_proxy(Server::bind("127.0.0.1:0").unwrap());

    let mut stream = Client::new(proxy).connect(echo).expect("connect");
    stream.write_all(b"x").unwrap();
    let mut byte = [0u8; 1];
    stream.read_exact(&mut byte).unwrap();
    assert_eq!(&byte, b"x");

    drop(stream);
    assert!(server.join().unwrap().is_ok());
}
