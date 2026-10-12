//! The unprivileged L4 backend — real UDP traffic on loopback in both directions (runs in
//! CI, no `CAP_NET_RAW`).
#![cfg(all(target_os = "linux", feature = "transport"))]

use rawsock::{RawIo, TransportSocket};
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// A loopback UDP peer with a read timeout, and its IPv4 address.
fn peer() -> (UdpSocket, SocketAddrV4) {
    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let SocketAddr::V4(addr) = peer.local_addr().unwrap() else {
        unreachable!("bound to an IPv4 address")
    };
    (peer, addr)
}

#[test]
fn transport_udp_loopback() {
    let (server, addr) = peer();

    let mut sock = TransportSocket::udp().unwrap();
    sock.connect(addr).unwrap();
    sock.send_raw(b"rawsock-l4").unwrap();

    let mut buf = [0u8; 64];
    let n = server.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"rawsock-l4");
}

#[test]
fn transport_recv_returns_the_peer_reply_verbatim() {
    let (peer, addr) = peer();
    let mut sock = TransportSocket::udp().unwrap();
    sock.connect(addr).unwrap();
    sock.send_raw(b"ping").unwrap();

    // The peer learns our ephemeral address from the request and answers it.
    let mut buf = [0u8; 64];
    let (_, from) = peer.recv_from(&mut buf).unwrap();
    peer.send_to(b"pong", from).unwrap();

    // TransportSocket has no read timeout, so receive on a thread and bound the wait here.
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = [0u8; 64];
        let got = sock.recv(&mut buf).map(|n| buf[..n].to_vec());
        let _ = tx.send(got);
    });
    let got = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("recv did not return within 2s")
        .unwrap();
    assert_eq!(got, b"pong");
}
