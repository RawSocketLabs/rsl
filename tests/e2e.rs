#![cfg(feature = "v5")]
//! End-to-end flows: a real client through a real proxy to a real target,
//! exercising CONNECT, BIND, UDP ASSOCIATE, and authenticated CONNECT.

mod common;

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::thread;
use std::time::Duration;

use socks::client::{Client, TargetAddr};
use socks::server::Server;

use common::{spawn_proxy, spawn_tcp_echo, spawn_udp_echo, user_pass_server};

#[test]
fn connect_relays_data() {
    let echo = spawn_tcp_echo();
    let (proxy, server) = spawn_proxy(Server::bind("127.0.0.1:0").unwrap());

    let mut stream = Client::new(proxy).connect(echo).expect("connect succeeds");
    stream.write_all(b"hello through socks").unwrap();
    let mut buf = [0u8; 19];
    stream.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"hello through socks");

    drop(stream);
    assert!(server.join().unwrap().is_ok());
}

#[test]
fn authenticated_connect_relays_data() {
    let echo = spawn_tcp_echo();
    let (proxy, server) = spawn_proxy(user_pass_server());

    let mut stream = Client::with_user_pass(proxy, "user", "hunter2")
        .unwrap()
        .connect(echo)
        .expect("authenticated connect succeeds");
    stream.write_all(b"ping").unwrap();
    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"ping");

    drop(stream);
    assert!(server.join().unwrap().is_ok());
}

#[test]
fn bind_accepts_and_relays_peer() {
    let (proxy, server) = spawn_proxy(Server::bind("127.0.0.1:0").unwrap());

    let listener = Client::new(proxy)
        .bind(SocketAddr::from(([0, 0, 0, 0], 0)))
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
    assert_eq!(peer_addr.ip(), IpAddr::from([127, 0, 0, 1]));
    let mut buf = [0u8; 9];
    stream.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"from peer");
    stream.write_all(b"to peer").unwrap();
    assert_eq!(&peer.join().unwrap(), b"to peer");

    drop(stream);
    assert!(server.join().unwrap().is_ok());
}

#[test]
fn udp_associate_relays_datagrams() {
    let echo = spawn_udp_echo();
    let (proxy, server) = spawn_proxy(Server::bind("127.0.0.1:0").unwrap());

    let tunnel = Client::new(proxy).udp_associate().expect("associate succeeds");
    tunnel.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tunnel.send_to(echo, b"ping").expect("send succeeds");

    let mut buf = [0u8; 16];
    let (read, source) = tunnel.recv_from(&mut buf).expect("reply received");
    assert_eq!(&buf[..read], b"ping");
    assert_eq!(source, TargetAddr::Ip(echo));

    drop(tunnel);
    assert!(server.join().unwrap().is_ok());
}
