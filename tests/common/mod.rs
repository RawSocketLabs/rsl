//! Shared helpers for the integration/e2e/regression test binaries.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::thread;

#[allow(unused_imports)]
use socks::error::Result;

/// A TCP echo listener that serves one connection, mirroring bytes back.
/// Version-agnostic — both the SOCKS4 and SOCKS5 suites use it.
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
#[cfg(feature = "v5")]
pub fn spawn_udp_echo() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = socket.local_addr().unwrap();
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        if let Ok((read, source)) = socket.recv_from(&mut buf) {
            let _ = socket.send_to(&buf[..read], source);
        }
    });
    addr
}

/// SOCKS5 helpers. Each test binary uses a subset, so the glob re-export can
/// appear unused in some of them.
#[cfg(feature = "v5")]
#[allow(unused_imports)]
pub use v5_helpers::*;

#[cfg(feature = "v5")]
mod v5_helpers {
    use super::*;
    use socks::auth::UserPassAuthenticator;
    use socks::server::Server;

    /// Spawns a server that handles exactly one client; returns its address and
    /// a join handle yielding the session result.
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

    /// A minimal raw "proxy" that completes NoAuth negotiation, drains the
    /// request, and returns a reply carrying reply code `rep` — for exercising
    /// the client's reply-code handling without a real backend.
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
}

/// SOCKS4 helpers. Each test binary uses a subset, so the glob re-export can
/// appear unused in some of them.
#[cfg(feature = "v4")]
#[allow(unused_imports)]
pub use v4_helpers::*;

#[cfg(feature = "v4")]
mod v4_helpers {
    use super::*;
    use socks::server::v4::Server as V4Server;

    /// Spawns a SOCKS4 server that handles exactly one client.
    pub fn spawn_v4_proxy(server: V4Server) -> (SocketAddr, thread::JoinHandle<Result<()>>) {
        let addr = server.local_addr().unwrap();
        let handle = thread::spawn(move || server.accept());
        (addr, handle)
    }

    /// A minimal raw "proxy" that reads one SOCKS4 request (head + userid, and
    /// — when the `0.0.0.x` marker is present — a trailing domain) and returns
    /// an 8-byte reply carrying code `cd`, without a real backend. Used to
    /// exercise the client's reply-code handling.
    pub fn fake_v4_proxy_replying(cd: u8) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut head = [0u8; 8];
            if s.read_exact(&mut head).is_err() {
                return;
            }
            // Drain the NULL-terminated userid.
            read_until_null(&mut s);
            // If DSTIP is the 0.0.0.x marker, a domain string follows.
            if head[4] == 0 && head[5] == 0 && head[6] == 0 && head[7] != 0 {
                read_until_null(&mut s);
            }
            let _ = s.write_all(&[0, cd, 0, 0, 0, 0, 0, 0]);
        });
        addr
    }

    fn read_until_null(s: &mut std::net::TcpStream) {
        let mut byte = [0u8; 1];
        while s.read_exact(&mut byte).is_ok() && byte[0] != 0 {}
    }
}
