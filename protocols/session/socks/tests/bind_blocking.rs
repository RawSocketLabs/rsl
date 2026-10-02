//! BIND's two replies, authorization boundary, and owned TCP deadlines.
#[expect(
    dead_code,
    reason = "Shared fixtures also serve CONNECT and asynchronous suites."
)]
mod support;

use socks::{
    error::Error,
    server::policy::ServerAuth,
    v5::{
        Endpoint, ReplyCode, auth::ClientAuth, client::blocking as client,
        server::blocking as server,
    },
};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    time::Duration,
};
use support::Script;

// Independent RFC 1928 §§4,6 bytes: BIND hint 127.0.0.1:0, listener :1080,
// connecting peer 192.0.2.1:443. The request's zero port is not CONNECT policy.
const BIND: &[u8] = &[5, 2, 0, 1, 127, 0, 0, 1, 0, 0];
const LISTEN: &[u8] = &[5, 0, 0, 1, 127, 0, 0, 1, 4, 56];
const PEER: &[u8] = &[5, 0, 0, 1, 192, 0, 2, 1, 1, 187];
const DENIED: &[u8] = &[5, 2, 0, 1, 0, 0, 0, 0, 0, 0];

fn hint() -> Endpoint {
    SocketAddr::from(([127, 0, 0, 1], 0)).into()
}

#[test]
fn second_reply_partial_write_does_not_attempt_a_failure_reply() {
    struct Limited {
        script: Script,
        remaining: usize,
    }
    impl Read for Limited {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.script.read(bytes)
        }
    }
    impl Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::ErrorKind::BrokenPipe.into());
            }
            let count = self
                .script
                .write(&bytes[..bytes.len().min(self.remaining)])?;
            self.remaining -= count;
            Ok(count)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.script.flush()
        }
    }
    let mut io = Limited {
        script: Script::new([&[5, 1, 0][..], BIND].concat(), usize::MAX),
        remaining: usize::MAX,
    };
    let mut waiting = server::exchange_bind(&mut io, &ServerAuth::no_authentication())
        .unwrap()
        .send_bound(listener_addr())
        .unwrap();
    waiting.get_mut().remaining = 3;
    assert!(
        matches!(waiting.send_success(peer_addr(), |_|true),Err(Error::Io(e)) if e.kind()==std::io::ErrorKind::BrokenPipe)
    );
    assert_eq!(
        io.script.written,
        [&[5, 0][..], LISTEN, &PEER[..3]].concat()
    );
}

#[test]
fn invalid_headers_and_failure_codes_are_checked_at_both_replies() {
    for first in [true, false] {
        for (offset, value) in [(0, 4), (2, 1)] {
            let mut bad = PEER.to_vec();
            bad[offset] = value;
            let input = if first {
                [&[5, 0][..], &bad].concat()
            } else {
                [&[5, 0][..], LISTEN, &bad].concat()
            };
            let result = client::bind(Script::new(input, 1), hint(), ClientAuth::NoAuthentication)
                .and_then(client::Binding::wait_for_peer);
            match offset {
                0 => assert!(matches!(
                    result,
                    Err(Error::Version {
                        expected: 5,
                        actual: 4
                    })
                )),
                _ => assert!(matches!(result, Err(Error::Reserved(1)))),
            }
        }
    }
    for code in [ReplyCode::Succeeded, ReplyCode::Other(0)] {
        for first in [true, false] {
            let mut script = Script::new([&[5, 1, 0][..], BIND].concat(), 1);
            let request =
                server::exchange_bind(&mut script, &ServerAuth::no_authentication()).unwrap();
            let result = if first {
                request.send_failure(code)
            } else {
                request
                    .send_bound(listener_addr())
                    .unwrap()
                    .send_failure(code)
            };
            assert!(matches!(result, Err(Error::Reply(_))));
            assert_eq!(
                script.written,
                if first {
                    vec![5, 0]
                } else {
                    [&[5, 0][..], LISTEN].concat()
                }
            );
        }
    }
}

#[test]
fn owned_tcp_denied_stray_peer_is_closed_and_the_anticipated_peer_is_accepted() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = proxy.local_addr().unwrap();
    let (stray_tx, stray_rx) = std::sync::mpsc::channel::<SocketAddr>();
    std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let (control, _) = proxy.accept().unwrap();
            let request = server::exchange_bind_tcp(
                control,
                &ServerAuth::no_authentication(),
                Duration::from_secs(5),
            )
            .unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let waiting = request
                .listen(listener, addr, Duration::from_secs(5))
                .unwrap();
            let stray = stray_rx.recv().unwrap();
            let mut rejected = 0;
            let (_control, _incoming) = waiting
                .accept(|peer| {
                    let allowed = peer != stray;
                    rejected += usize::from(!allowed);
                    allowed
                })
                .unwrap();
            assert_eq!(rejected, 1);
        });
        let binding = client::bind_tcp(
            address,
            hint(),
            ClientAuth::NoAuthentication,
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .unwrap();
        let Endpoint::Ipv4 { address, port } = *binding.bound() else {
            panic!("numeric address");
        };
        let listener = SocketAddr::from((address, port));

        // A stray connection arrives first and must be closed, not end the BIND.
        let mut stray = TcpStream::connect(listener).unwrap();
        stray
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stray_tx.send(stray.local_addr().unwrap()).unwrap();
        match stray.read(&mut [0]) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) => {}
            other => panic!("denied peer must close, not remain pending: {other:?}"),
        }

        let peer = TcpStream::connect(listener).unwrap();
        let (_stream, reported) = binding.wait_for_peer().unwrap();
        assert_eq!(reported, Endpoint::from(peer.local_addr().unwrap()));
        worker.join().unwrap();
    });
}

#[test]
fn owned_tcp_server_keeps_client_bytes_sent_before_the_second_reply() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_addr = proxy.local_addr().unwrap();
    let (listener_tx, listener_rx) = std::sync::mpsc::channel::<SocketAddr>();
    std::thread::scope(|scope| {
        let server = scope.spawn(move || {
            let (control, _) = proxy.accept().unwrap();
            let request = server::exchange_bind_tcp(
                control,
                &ServerAuth::no_authentication(),
                Duration::from_secs(5),
            )
            .unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let advertised = listener.local_addr().unwrap();
            listener_tx.send(advertised).unwrap();
            let waiting = request
                .listen(listener, advertised, Duration::from_secs(5))
                .unwrap();
            let (mut control, _incoming) = waiting.accept(|_| true).unwrap();
            let mut bytes = [0; 5];
            control.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, b"early");
        });
        // Raw client: greeting, BIND request, and application bytes in one write.
        let mut control = TcpStream::connect(proxy_addr).unwrap();
        control
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        control
            .write_all(&[&[5, 1, 0][..], BIND, b"early"].concat())
            .unwrap();
        let mut replies = [0; 2 + 10];
        control.read_exact(&mut replies).unwrap();
        assert_eq!(&replies[..2], &[5, 0]);
        let _peer = TcpStream::connect(listener_rx.recv().unwrap()).unwrap();
        let mut second = [0; 10];
        control.read_exact(&mut second).unwrap();
        assert_eq!(second[1], 0, "second reply must succeed");
        server.join().unwrap();
    });
}

#[test]
fn owned_tcp_client_keeps_bytes_coalesced_with_the_second_reply() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_addr = proxy.local_addr().unwrap();
    std::thread::scope(|scope| {
        let server = scope.spawn(move || {
            // Raw server: the second reply and application bytes arrive in one write.
            let (mut control, _) = proxy.accept().unwrap();
            let mut greeting = [0; 3];
            control.read_exact(&mut greeting).unwrap();
            assert_eq!(greeting, [5, 1, 0]);
            control.write_all(&[5, 0]).unwrap();
            let mut request = [0; 10];
            control.read_exact(&mut request).unwrap();
            assert_eq!(request, BIND);
            control.write_all(LISTEN).unwrap();
            control.write_all(&[PEER, b"early"].concat()).unwrap();
        });
        let binding = client::bind_tcp(
            proxy_addr,
            hint(),
            ClientAuth::NoAuthentication,
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .unwrap();
        let (mut stream, reported) = binding.wait_for_peer().unwrap();
        assert_eq!(reported, peer_addr().into());
        assert_eq!(stream.get_ref().read_timeout().unwrap(), None);
        let mut bytes = [0; 5];
        stream.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"early");
        server.join().unwrap();
    });
}

#[test]
fn invalid_tcp_budgets_fail_before_opening_a_proxy_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    for (handshake, peer) in [
        (Duration::ZERO, Duration::from_secs(1)),
        (Duration::from_secs(1), Duration::ZERO),
    ] {
        assert!(matches!(
            client::bind_tcp(
                listener.local_addr().unwrap(),
                hint(),
                ClientAuth::NoAuthentication,
                handshake,
                peer
            ),
            Err(Error::InvalidLimits)
        ));
        assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    }
}
fn listener_addr() -> SocketAddr {
    ([127, 0, 0, 1], 1080).into()
}
fn peer_addr() -> SocketAddr {
    ([192, 0, 2, 1], 443).into()
}

#[test]
fn client_preserves_both_replies_and_coalesced_application_bytes() {
    for chunk in [1, 2, usize::MAX] {
        let mut script = Script::new([&[5, 0][..], LISTEN, PEER, b"payload"].concat(), chunk);
        let binding = client::bind(&mut script, hint(), ClientAuth::NoAuthentication).unwrap();
        assert_eq!(binding.bound(), &Endpoint::from(listener_addr()));
        let (mut stream, peer) = binding.wait_for_peer().unwrap();
        assert_eq!(peer, Endpoint::from(peer_addr()));
        let mut payload = Vec::new();
        stream.read_to_end(&mut payload).unwrap();
        assert_eq!(payload, b"payload");
        drop(stream);
        assert_eq!(script.written, [&[5, 1, 0][..], BIND].concat());
    }
}

#[test]
fn server_keeps_early_payload_and_authorizes_the_second_reply() {
    for chunk in [1, usize::MAX] {
        let mut script = Script::new([&[5, 1, 0][..], BIND, b"early"].concat(), chunk);
        let request = server::exchange_bind(&mut script, &ServerAuth::no_authentication()).unwrap();
        assert_eq!(request.destination(), &hint());
        let waiting = request.send_bound(listener_addr()).unwrap();
        assert_eq!(waiting.destination(), &hint());
        let mut called = false;
        let mut stream = waiting
            .send_success(peer_addr(), |peer| {
                called = true;
                assert_eq!(peer, peer_addr());
                true
            })
            .unwrap();
        assert!(called);
        let mut payload = Vec::new();
        stream.read_to_end(&mut payload).unwrap();
        assert_eq!(payload, b"early");
        drop(stream);
        assert_eq!(script.written, [&[5, 0][..], LISTEN, PEER].concat());
    }
}

#[test]
fn denied_inbound_peer_sends_only_a_failing_second_reply() {
    let mut script = Script::new([&[5, 1, 0][..], BIND].concat(), 1);
    let waiting = server::exchange_bind(&mut script, &ServerAuth::no_authentication())
        .unwrap()
        .send_bound(listener_addr())
        .unwrap();
    assert!(matches!(
        waiting.send_success(peer_addr(), |_| false),
        Err(Error::PermissionDenied)
    ));
    assert_eq!(script.written, [&[5, 0][..], LISTEN, DENIED].concat());
}

#[test]
fn first_and_second_failure_replies_never_release_the_client_stream() {
    for first in [true, false] {
        let input = if first {
            [&[5, 0][..], DENIED].concat()
        } else {
            [&[5, 0][..], LISTEN, DENIED].concat()
        };
        let result = client::bind(Script::new(input, 1), hint(), ClientAuth::NoAuthentication)
            .and_then(client::Binding::wait_for_peer);
        assert!(matches!(
            result,
            Err(Error::Reply(ReplyCode::ConnectionNotAllowed))
        ));
    }
}

#[test]
fn truncation_at_either_reply_is_terminal() {
    for first in [true, false] {
        for end in 0..LISTEN.len() {
            let input = if first {
                [&[5, 0][..], &LISTEN[..end]].concat()
            } else {
                [&[5, 0][..], LISTEN, &PEER[..end]].concat()
            };
            assert!(
                client::bind(Script::new(input, 1), hint(), ClientAuth::NoAuthentication)
                    .and_then(client::Binding::wait_for_peer)
                    .is_err(),
                "first {first}, end {end}"
            );
        }
    }
}

#[test]
fn bind_reuses_username_password_authentication_without_exposing_a_stream_early() {
    let mut script = Script::new([&[5, 2, 1, 0][..], LISTEN, PEER].concat(), 1);
    client::bind(
        &mut script,
        hint(),
        ClientAuth::UsernamePassword {
            user: b"u",
            pass: b"p",
        },
    )
    .unwrap()
    .wait_for_peer()
    .unwrap();
    assert_eq!(
        script.written,
        [&[5, 1, 2, 1, 1, b'u', 1, b'p'][..], BIND].concat()
    );
    let mut script = Script::new([&[5, 1, 2, 1, 1, b'u', 1, b'p'][..], BIND].concat(), 1);
    let auth = ServerAuth::username_password(|u, p| u == b"u" && p == b"p");
    let request = server::exchange_bind(&mut script, &auth).unwrap();
    assert_eq!(
        request.authentication(),
        socks::server::policy::Authentication::UsernamePassword
    );
    request
        .send_failure(ReplyCode::ConnectionNotAllowed)
        .unwrap();
    assert_eq!(script.written, [&[5, 2, 1, 0][..], DENIED].concat());
}

#[test]
fn bind_is_opt_in_and_invalid_local_hints_write_nothing() {
    let mut script = Script::new([&[5, 1, 0][..], BIND].concat(), 1);
    assert!(matches!(
        server::exchange(&mut script, &ServerAuth::no_authentication()),
        Err(Error::UnsupportedCommand(socks::v5::Command::Bind))
    ));
    let mut connect = BIND.to_vec();
    connect[1] = 1;
    let mut script = Script::new([&[5, 1, 0][..], &connect].concat(), 1);
    assert!(matches!(
        server::exchange_bind(&mut script, &ServerAuth::no_authentication()),
        Err(Error::UnsupportedCommand(socks::v5::Command::Connect))
    ));
    let mut script = Script::new([], 1);
    assert!(matches!(
        client::bind(
            &mut script,
            Endpoint::domain_raw([], 0),
            ClientAuth::NoAuthentication
        ),
        Err(Error::InvalidDomain(socks::v5::DomainError::Empty))
    ));
    assert_eq!(script.written, b"");
}

#[test]
fn owned_tcp_bind_accepts_a_peer_and_clears_protocol_timeouts() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_addr = proxy.local_addr().unwrap();
    std::thread::scope(|scope| {
        let server = scope.spawn(move || {
            let (control, _) = proxy.accept().unwrap();
            let request = server::exchange_bind_tcp(
                control,
                &ServerAuth::no_authentication(),
                Duration::from_secs(5),
            )
            .unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let advertised = listener.local_addr().unwrap();
            let waiting = request
                .listen(listener, advertised, Duration::from_secs(5))
                .unwrap();
            let (mut control, mut incoming) =
                waiting.accept(|peer| peer.ip().is_loopback()).unwrap();
            assert_eq!(control.get_ref().read_timeout().unwrap(), None);
            assert_eq!(control.get_ref().write_timeout().unwrap(), None);
            let mut bytes = [0; 5];
            incoming.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, b"hello");
            control.write_all(b"world").unwrap();
        });
        let binding = client::bind_tcp(
            proxy_addr,
            hint(),
            ClientAuth::NoAuthentication,
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .unwrap();
        let Endpoint::Ipv4 { address, port } = *binding.bound() else {
            panic!("numeric listener");
        };
        let mut peer = TcpStream::connect(SocketAddr::from((address, port))).unwrap();
        peer.write_all(b"hello").unwrap();
        let (mut stream, reported) = binding.wait_for_peer().unwrap();
        assert_eq!(reported, Endpoint::from(peer.local_addr().unwrap()));
        assert_eq!(stream.get_ref().read_timeout().unwrap(), None);
        let mut bytes = [0; 5];
        stream.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"world");
        server.join().unwrap();
    });
}

#[test]
fn owned_accept_timeout_sends_a_second_failure_without_authorizing() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    std::thread::scope(|scope| {
        let client = TcpStream::connect(proxy.local_addr().unwrap()).unwrap();
        let (control, _) = proxy.accept().unwrap();
        let worker = scope.spawn(move || {
            let request = server::exchange_bind_tcp(
                control,
                &ServerAuth::no_authentication(),
                Duration::from_secs(5),
            )
            .unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let waiting = request
                .listen(listener, addr, Duration::from_millis(10))
                .unwrap();
            let result = waiting.accept(|_| panic!("no peer to authorize"));
            assert!(matches!(
                result,
                Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::TimedOut
            ));
        });
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let binding = client::bind(client, hint(), ClientAuth::NoAuthentication).unwrap();
        assert!(matches!(
            binding.wait_for_peer(),
            Err(Error::Reply(ReplyCode::GeneralFailure))
        ));
        worker.join().unwrap();
    });
}
