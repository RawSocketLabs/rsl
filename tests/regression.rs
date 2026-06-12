#![cfg(feature = "v5")]
//! Regression guards for specific defects found and fixed during hardening:
//! UDP source hijacking, peerless-BIND pinning, silent-client handshake hang,
//! and client-IP pinning of a UDP association to its control connection.

mod common;

use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use socks::client::{Client, TargetAddr};
use socks::server::Server;

use common::spawn_proxy;

//~ verifies rfc1928#7/must.977213
#[test]
fn udp_relay_drops_unsolicited_source() {
    let echo = UdpSocket::bind("127.0.0.1:0").unwrap();
    let echo_addr = echo.local_addr().unwrap();
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        if let Ok((read, src)) = echo.recv_from(&mut buf) {
            let _ = echo.send_to(&buf[..read], src);
        }
    });

    let (proxy, server) = spawn_proxy(Server::bind("127.0.0.1:0").unwrap());
    let tunnel = Client::new(proxy)
        .udp_associate()
        .expect("associate succeeds");
    tunnel
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    // Establish the association and confirm the legitimate path works.
    tunnel.send_to(echo_addr, b"hi").expect("send succeeds");
    let mut buf = [0u8; 16];
    let (n, _) = tunnel.recv_from(&mut buf).expect("echo reply received");
    assert_eq!(&buf[..n], b"hi");

    // A socket the client never contacted injects straight at the relay.
    // Its source is not the client and not a contacted target, so the relay
    // must drop it (RFC 1928 §7 must.977213) — the client sees nothing.
    let rogue = UdpSocket::bind("127.0.0.1:0").unwrap();
    rogue.send_to(b"INJECT", tunnel.relay_addr()).unwrap();
    let mut buf = [0u8; 16];
    assert!(
        tunnel.recv_from(&mut buf).is_err(),
        "unsolicited datagram must not reach the client"
    );

    drop(tunnel);
    assert!(server.join().unwrap().is_ok());
}

#[test]
fn udp_associate_pins_client_to_control_connection() {
    let echo = UdpSocket::bind("127.0.0.1:0").unwrap();
    let echo_addr = echo.local_addr().unwrap();
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        if let Ok((read, source)) = echo.recv_from(&mut buf) {
            let _ = echo.send_to(&buf[..read], source);
        }
    });

    let (proxy, server) = spawn_proxy(Server::bind("127.0.0.1:0").unwrap());
    let tunnel = Client::new(proxy)
        .udp_associate()
        .expect("associate succeeds");
    tunnel
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    // The client's UDP socket shares 127.0.0.1 with the control connection,
    // so its datagrams are accepted and relayed.
    tunnel.send_to(echo_addr, b"pinned").expect("send succeeds");
    let mut buf = [0u8; 16];
    let (read, source) = tunnel.recv_from(&mut buf).expect("reply received");
    assert_eq!(&buf[..read], b"pinned");
    assert_eq!(source, TargetAddr::Ip(echo_addr));

    drop(tunnel);
    assert!(server.join().unwrap().is_ok());
}

#[test]
fn bind_times_out_when_peer_never_connects() {
    // A BIND whose inbound peer never arrives must not pin the handler: the
    // server should give up near the bind deadline and reply failure.
    let server = Server::bind("127.0.0.1:0")
        .unwrap()
        .with_bind_timeout(Some(Duration::from_millis(300)));
    let (proxy, handle) = spawn_proxy(server);

    let listener = Client::new(proxy)
        .bind(SocketAddr::from(([0, 0, 0, 0], 0)))
        .expect("bind reply");
    let start = Instant::now();
    let result = listener.accept();

    assert!(result.is_err(), "peerless BIND should fail");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "server should give up near the bind deadline, not block"
    );
    assert!(handle.join().unwrap().is_err());
}

#[test]
fn handshake_times_out_on_silent_client() {
    // A client that connects but never sends the method identifier must not
    // hold the handler open: the server should error out promptly.
    let server = Server::bind("127.0.0.1:0")
        .unwrap()
        .with_handshake_timeout(Some(Duration::from_millis(200)));
    let (proxy, handle) = spawn_proxy(server);

    let _silent = TcpStream::connect(proxy).unwrap();
    let start = Instant::now();
    let result = handle.join().unwrap();

    assert!(result.is_err(), "silent client should time out");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "server should give up near the handshake deadline, not block"
    );
}
