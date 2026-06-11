//! Shared helpers for the integration/e2e/regression test binaries.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::thread;

use socks::auth::UserPassAuthenticator;
use socks::error::Result;
use socks::server::Server;

/// A TCP echo listener that serves one connection, mirroring bytes back.
pub fn spawn_tcp_echo() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            while let Ok(read) = stream.read(&mut buf) {
                if read == 0 || stream.write_all(&buf[..read]).is_err() {
                    break;
                }
            }
        }
    });
    addr
}

/// A UDP echo socket that mirrors one datagram back to its sender.
pub fn spawn_udp_echo() -> SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = socket.local_addr().unwrap();
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        if let Ok((read, source)) = socket.recv_from(&mut buf) {
            let _ = socket.send_to(&buf[..read], source);
        }
    });
    addr
}

/// Spawns a server that handles exactly one client; returns its address and a
/// join handle yielding the session result.
pub fn spawn_proxy(server: Server) -> (SocketAddr, thread::JoinHandle<Result<()>>) {
    let addr = server.local_addr().unwrap();
    let handle = thread::spawn(move || server.accept());
    (addr, handle)
}

/// A proxy that accepts only the username/password method `user`/`hunter2`.
pub fn user_pass_server() -> Server {
    Server::bind("127.0.0.1:0")
        .unwrap()
        .with_authenticators(vec![Box::new(UserPassAuthenticator::new(|user, pass| {
            user == b"user" && pass == b"hunter2"
        }))])
}

/// A minimal raw "proxy" that completes NoAuth negotiation, drains the request,
/// and returns a reply carrying reply code `rep` — for exercising the client's
/// reply-code handling without a real backend.
pub fn fake_proxy_replying(rep: u8) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut head = [0u8; 2];
        if s.read_exact(&mut head).is_err() {
            return;
        }
        let mut methods = vec![0u8; head[1] as usize];
        let _ = s.read_exact(&mut methods);
        let _ = s.write_all(&[5, 0x00]); // offer NoAuth
        let mut rhead = [0u8; 4];
        if s.read_exact(&mut rhead).is_err() {
            return;
        }
        let addr_len = match rhead[3] {
            1 => 4,
            4 => 16,
            3 => {
                let mut len = [0u8; 1];
                s.read_exact(&mut len).unwrap();
                len[0] as usize
            }
            _ => 0,
        };
        let mut rest = vec![0u8; addr_len + 2];
        let _ = s.read_exact(&mut rest);
        let _ = s.write_all(&[5, rep, 0, 1, 0, 0, 0, 0, 0, 0]);
    });
    addr
}
