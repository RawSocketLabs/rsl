//! The unprivileged L4 backend — a real UDP round-trip on loopback (runs in CI, no
//! `CAP_NET_RAW`). The privileged L3/L2 backends and their namespace-gated tests land
//! with those backends.
#![cfg(all(target_os = "linux", feature = "transport"))]

use rawsock::{RawIo, TransportSocket};
use std::net::UdpSocket;
use std::time::Duration;

#[test]
fn transport_udp_loopback() {
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let addr = match server.local_addr().unwrap() {
        std::net::SocketAddr::V4(a) => a,
        std::net::SocketAddr::V6(_) => unreachable!(),
    };

    let mut sock = TransportSocket::udp().unwrap();
    sock.connect(addr).unwrap();
    sock.send_raw(b"rawsock-l4").unwrap();

    let mut buf = [0u8; 64];
    let n = server.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"rawsock-l4");
}
