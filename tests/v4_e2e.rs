#![cfg(feature = "v4")]
//! End-to-end SOCKS4 / 4A flows: a real client through a real proxy to a real
//! target, exercising CONNECT, BIND, reply-code error mapping, the authorizer
//! hook, the raw escape hatch, and (under `v4a`) domain-name resolution.

mod common;

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::thread;
use std::time::Duration;

use socks::client::v4::{raw, Client};
use socks::error::SocksError;
use socks::server::v4::Server;
use socks::v4::ReplyCode;

use common::{fake_v4_proxy_replying, spawn_tcp_echo, spawn_v4_proxy};

/// Extracts the IPv4 address an echo listener bound to.
fn v4(addr: std::net::SocketAddr) -> SocketAddrV4 {
    match addr {
        std::net::SocketAddr::V4(a) => a,
        std::net::SocketAddr::V6(_) => panic!("expected IPv4 echo address"),
    }
}

#[test]
fn connect_relays_data() {
    let echo = v4(spawn_tcp_echo());
    let (proxy, server) = spawn_v4_proxy(Server::bind("127.0.0.1:0").unwrap());

    let mut stream = Client::new(proxy)
        .userid("alice")
        .connect((*echo.ip(), echo.port()))
        .expect("connect succeeds");
    stream.write_all(b"hello through socks4").unwrap();
    let mut buf = [0u8; 20];
    stream.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"hello through socks4");

    drop(stream);
    assert!(server.join().unwrap().is_ok());
}

#[test]
fn rejected_reply_maps_to_v4_reply_failure() {
    let proxy = fake_v4_proxy_replying(91); // 91 = request rejected/failed
    let err = Client::new(proxy)
        .connect((Ipv4Addr::new(127, 0, 0, 1), 9))
        .expect_err("a 91 reply must surface as an error");
    assert!(
        matches!(err, SocksError::V4ReplyFailure(ReplyCode::Rejected)),
        "got {err:?}"
    );
}

#[test]
fn authorizer_rejection_is_reported() {
    let (proxy, server) = spawn_v4_proxy(
        Server::bind("127.0.0.1:0")
            .unwrap()
            .with_authorizer(|req| req.userid.to_string() == "alice"),
    );

    // "mallory" is not "alice": the server rejects with code 91.
    let err = Client::new(proxy)
        .userid("mallory")
        .connect((Ipv4Addr::new(127, 0, 0, 1), 9))
        .expect_err("unauthorized userid must be rejected");
    assert!(
        matches!(err, SocksError::V4ReplyFailure(ReplyCode::Rejected)),
        "got {err:?}"
    );
    // The server's handler returns the validation error it raised.
    assert!(server.join().unwrap().is_err());
}

#[test]
fn bind_accepts_and_relays_peer() {
    let (proxy, server) = spawn_v4_proxy(Server::bind("127.0.0.1:0").unwrap());

    // 0.0.0.0 means "don't pin the peer address".
    let listener = Client::new(proxy)
        .bind((Ipv4Addr::UNSPECIFIED, 0))
        .expect("bind succeeds");
    let bound = listener.bound;

    let peer = thread::spawn(move || {
        let mut stream = TcpStream::connect(bound).unwrap();
        stream.write_all(b"from peer").unwrap();
        let mut buf = [0u8; 7];
        stream.read_exact(&mut buf).unwrap();
        buf
    });

    let (mut stream, peer_addr) = listener.accept().expect("peer connects");
    assert_eq!(*peer_addr.ip(), Ipv4Addr::new(127, 0, 0, 1));
    let mut buf = [0u8; 9];
    stream.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"from peer");
    stream.write_all(b"to peer").unwrap();
    assert_eq!(&peer.join().unwrap(), b"to peer");

    drop(stream);
    assert!(server.join().unwrap().is_ok());
}

#[test]
fn raw_client_wrong_version_is_rejected_by_server() {
    let server = Server::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let handle = thread::spawn(move || server.accept());

    let client = raw::RawClient::connect(addr).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .write_request(&raw::malformed::wrong_version_request(
            Ipv4Addr::new(127, 0, 0, 1),
            9,
        ))
        .unwrap();
    // The server rejects a non-4 VN: it replies with code 91 then drops.
    let reply = client.read_reply().expect("server replies before closing");
    assert_eq!(reply.code, ReplyCode::Rejected);
    assert!(matches!(
        handle.join().unwrap(),
        Err(SocksError::UnsupportedVersion(5))
    ));
}

#[cfg(feature = "v4a")]
#[test]
fn socks4a_domain_connect_relays_data() {
    let echo = v4(spawn_tcp_echo());
    let (proxy, server) = spawn_v4_proxy(Server::bind("127.0.0.1:0").unwrap());

    // The proxy resolves "localhost" itself; the echo listener is on
    // 127.0.0.1, so the resolved connection reaches it.
    let mut stream = Client::new(proxy)
        .connect_domain("localhost", echo.port())
        .expect("4A domain connect succeeds");
    stream.write_all(b"socks4a").unwrap();
    let mut buf = [0u8; 7];
    stream.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"socks4a");

    drop(stream);
    assert!(server.join().unwrap().is_ok());
}

#[cfg(feature = "v4a")]
#[test]
fn socks4a_rejects_empty_or_oversized_host() {
    let client = Client::new("127.0.0.1:1080".parse().unwrap());
    assert!(matches!(
        client.connect_domain("", 80),
        Err(SocksError::Validation(_))
    ));
    assert!(matches!(
        client.connect_domain(&"x".repeat(256), 80),
        Err(SocksError::Validation(_))
    ));
}
