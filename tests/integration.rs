#![cfg(feature = "v5")]
//! Client/server integration: negotiation, error mapping, and the server's
//! handling of malformed or unacceptable input.

mod common;

use std::io::Write;
use std::net::{TcpListener, TcpStream};

use socks::client::Client;
use socks::error::SocksError;
use socks::server::Server;
use socks::v5::Response;

use common::{fake_proxy_replying, spawn_proxy, user_pass_server};

#[test]
fn connect_refused_maps_to_reply_failure() {
    // Bind-then-drop yields a port with no listener.
    let unused = TcpListener::bind("127.0.0.1:0").unwrap();
    let target = unused.local_addr().unwrap();
    drop(unused);

    let (proxy, server) = spawn_proxy(Server::bind("127.0.0.1:0").unwrap());
    let result = Client::new(proxy).connect(target);

    assert!(matches!(result, Err(SocksError::ReplyFailure(_))));
    assert!(server.join().unwrap().is_err());
}

#[test]
fn client_maps_specific_failure_reply_code() {
    let proxy = fake_proxy_replying(0x05); // ConnectionRefused
    let result = Client::new(proxy).connect(("example.com", 80));
    assert!(matches!(
        result,
        Err(SocksError::ReplyFailure(Response::ConnectionRefused))
    ));
}

#[test]
fn user_pass_rejection() {
    let (proxy, server) = spawn_proxy(user_pass_server());
    let result = Client::with_user_pass(proxy, "user", "wrong")
        .unwrap()
        .connect(("unused.invalid", 80));

    assert!(matches!(result, Err(SocksError::AuthenticationFailed)));
    assert!(matches!(
        server.join().unwrap(),
        Err(SocksError::AuthenticationFailed)
    ));
}

#[test]
fn no_acceptable_methods() {
    let (proxy, server) = spawn_proxy(user_pass_server());
    let result = Client::new(proxy).connect(("unused.invalid", 80));

    assert!(matches!(result, Err(SocksError::NoAcceptableMethods)));
    assert!(matches!(
        server.join().unwrap(),
        Err(SocksError::NoAcceptableMethods)
    ));
}

#[test]
fn rejects_non_v5_identifier() {
    let (proxy, handle) = spawn_proxy(Server::bind("127.0.0.1:0").unwrap());
    let mut stream = TcpStream::connect(proxy).unwrap();
    stream.write_all(&[4, 1, 0]).unwrap(); // SOCKS version 4
    assert!(matches!(
        handle.join().unwrap(),
        Err(SocksError::UnsupportedVersion(4))
    ));
}

#[test]
fn errors_on_truncated_identifier() {
    let (proxy, handle) = spawn_proxy(Server::bind("127.0.0.1:0").unwrap());
    let mut stream = TcpStream::connect(proxy).unwrap();
    stream.write_all(&[5, 2]).unwrap(); // claims 2 methods, sends none
    drop(stream); // half-close mid-message
    assert!(handle.join().unwrap().is_err());
}
