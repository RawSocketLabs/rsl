//! Protocol-derived transcript tests, with fragmented/coalesced I/O and loopback relay.

mod support;

use socks::blocking;
use socks::error::Error;
use socks::session::{ClientAuth, ServerAuth, ServerConfig};
use socks::v5::{Endpoint, ReplyCode};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use support::{REPLY, REQUEST, Script};

fn target() -> Endpoint {
    SocketAddr::from(([127, 0, 0, 1], 1080)).into()
}

#[test]
fn ipv6_and_max_domain_frames_preserve_tunnel_data() {
    for (destination, request) in support::variable_address_vectors() {
        let mut reply = request.clone();
        reply[1] = 0;
        let mut script = Script::new([b"\x05\x00".as_slice(), &reply, b"early"].concat(), 1);
        let (tunnel, bound) = blocking::connect(
            &mut script,
            destination.clone(),
            ClientAuth::NoAuthentication,
        )
        .unwrap();
        assert_eq!(bound, destination);
        let mut early = [0; 5];
        tunnel.read_exact(&mut early).unwrap();
        assert_eq!(&early, b"early");
        assert_eq!(
            script.written,
            [b"\x05\x01\x00".as_slice(), &request].concat()
        );
        let mut script = Script::new([b"\x05\x01\x00".as_slice(), &request, b"early"].concat(), 1);
        let pending = blocking::accept(&mut script, &ServerAuth::no_authentication()).unwrap();
        assert_eq!(pending.destination(), &destination);
        pending
            .accept(destination)
            .unwrap()
            .read_exact(&mut early)
            .unwrap();
        assert_eq!(&early, b"early");
        assert_eq!(script.written, [b"\x05\x00".as_slice(), &reply].concat());
    }
}

#[test]
fn client_golden_transcript_preserves_coalesced_tunnel_bytes() {
    for chunk in [1, 2, usize::MAX] {
        let input = [b"\x05\x00".as_slice(), REPLY, b"early"].concat();
        let mut script = Script::new(input, chunk);
        let (tunnel, bound) =
            blocking::connect(&mut script, target(), ClientAuth::NoAuthentication).unwrap();
        assert_eq!(bound, target());
        let mut early = Vec::new();
        tunnel.read_to_end(&mut early).unwrap();
        assert_eq!(early, b"early");
        assert_eq!(
            script.written,
            [b"\x05\x01\x00".as_slice(), REQUEST].concat()
        );
    }
}

#[test]
fn server_defers_success_until_caller_accepts_and_preserves_early_payload() {
    for chunk in [1, usize::MAX] {
        let mut script = Script::new(
            [b"\x05\x01\x00".as_slice(), REQUEST, b"early"].concat(),
            chunk,
        );
        let pending = blocking::accept(&mut script, &ServerAuth::no_authentication()).unwrap();
        assert_eq!(pending.destination(), &target());
        let tunnel = pending.accept(target()).unwrap();
        let mut payload = Vec::new();
        tunnel.read_to_end(&mut payload).unwrap();
        assert_eq!(payload, b"early");
        assert_eq!(script.written, [b"\x05\x00".as_slice(), REPLY].concat());
    }
    let mut script = Script::new([b"\x05\x01\x00".as_slice(), REQUEST].concat(), 1);
    blocking::accept(&mut script, &ServerAuth::no_authentication())
        .unwrap()
        .reject(ReplyCode::ConnectionNotAllowed)
        .unwrap();
    assert_eq!(script.written, [5, 0, 5, 2, 0, 1, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn username_password_transcript_and_no_downgrade() {
    let auth = ClientAuth::UsernamePassword {
        username: b"u",
        password: b"p",
    };
    let mut script = Script::new([b"\x05\x02\x01\x00".as_slice(), REPLY].concat(), 1);
    blocking::connect(&mut script, target(), auth).unwrap();
    assert_eq!(
        script.written,
        [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat()
    );
    let mut downgraded = Script::new([5, 0], 1);
    assert!(matches!(
        blocking::connect(&mut downgraded, target(), auth),
        Err(Error::UnexpectedMethod(_))
    ));
    assert_eq!(downgraded.written, [5, 1, 2]);
    let policy = ServerAuth::username_password(|user, pass| user == b"u" && pass == b"p");
    let mut script = Script::new(
        [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat(),
        1,
    );
    blocking::accept(&mut script, &policy)
        .unwrap()
        .accept(target())
        .unwrap();
    assert_eq!(
        script.written,
        [b"\x05\x02\x01\x00".as_slice(), REPLY].concat()
    );
    let mut no_auth = Script::new([5, 1, 0], 1);
    assert!(matches!(
        blocking::accept(&mut no_auth, &policy),
        Err(Error::NoAcceptableMethod)
    ));
    assert_eq!(no_auth.written, [5, 255]);
}

#[test]
fn authentication_failures_do_not_advance_to_command() {
    for credentials in [
        b"\x01\x01u\x01x".as_slice(),
        b"\x01\x00\x00",
        b"\x02\x01u\x01p",
    ] {
        let mut script = Script::new(
            [b"\x05\x01\x02".as_slice(), credentials, REQUEST].concat(),
            1,
        );
        let policy = ServerAuth::username_password(|u, p| u == b"u" && p == b"p");
        assert!(matches!(
            blocking::accept(&mut script, &policy),
            Err(Error::AuthenticationRejected)
        ));
        assert_eq!(script.written, [5, 2, 1, 1]);
        assert_eq!(script.incoming.position(), (3 + credentials.len()) as u64);
    }
    for status in 1..=255 {
        let mut script = Script::new([5, 2, 1, status], 1);
        let auth = ClientAuth::UsernamePassword {
            username: b"u",
            password: b"p",
        };
        assert!(matches!(
            blocking::connect(&mut script, target(), auth),
            Err(Error::AuthenticationRejected)
        ));
        assert_eq!(script.written, b"\x05\x01\x02\x01\x01u\x01p");
    }
}

#[test]
fn invalid_local_credentials_emit_nothing_and_max_lengths_work() {
    for (user, pass) in [
        (vec![], vec![1]),
        (vec![1], vec![]),
        (vec![1; 256], vec![1]),
        (vec![1], vec![1; 256]),
    ] {
        let mut script = Script::new(Vec::new(), 1);
        assert!(
            blocking::connect(
                &mut script,
                target(),
                ClientAuth::UsernamePassword {
                    username: &user,
                    password: &pass
                }
            )
            .is_err()
        );
        assert!(script.written.is_empty());
    }
    let mut script = Script::new([b"\x05\x02\x01\x00".as_slice(), REPLY].concat(), 1);
    blocking::connect(
        &mut script,
        target(),
        ClientAuth::UsernamePassword {
            username: &[7; 255],
            password: &[8; 255],
        },
    )
    .unwrap();
    assert_eq!(&script.written[..5], &[5, 1, 2, 1, 255]);
    assert_eq!(script.written[260], 255);
}

#[test]
fn all_truncated_handshake_prefixes_fail_without_panicking() {
    let client = [b"\x05\x02\x01\x00".as_slice(), REPLY].concat();
    for end in 0..client.len() {
        assert!(
            blocking::connect(
                Script::new(client[..end].to_vec(), 1),
                target(),
                ClientAuth::UsernamePassword {
                    username: b"u",
                    password: b"p"
                }
            )
            .is_err(),
            "prefix {end}"
        );
    }
    let server = [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat();
    for end in 0..server.len() {
        assert!(
            blocking::accept(
                Script::new(server[..end].to_vec(), 1),
                &ServerAuth::username_password(|_, _| true)
            )
            .is_err(),
            "prefix {end}"
        );
    }
}

#[test]
fn session_rejects_invalid_versions_reserved_commands_and_address_types() {
    for (offset, byte, code) in [
        (0, 4, 1),
        (1, 2, 7),
        (1, 3, 7),
        (1, 99, 7),
        (2, 1, 1),
        (3, 99, 8),
    ] {
        let mut request = REQUEST.to_vec();
        request[offset] = byte;
        let mut script = Script::new([b"\x05\x01\x00".as_slice(), &request].concat(), 1);
        assert!(blocking::accept(&mut script, &ServerAuth::no_authentication()).is_err());
        assert_eq!(script.written[3], code, "offset {offset}, byte {byte}");
    }
    for (offset, byte) in [(0, 4), (2, 1), (1, 255), (3, 99)] {
        let mut reply = REPLY.to_vec();
        reply[offset] = byte;
        assert!(
            blocking::connect(
                Script::new([b"\x05\x00".as_slice(), &reply].concat(), 1),
                target(),
                ClientAuth::NoAuthentication
            )
            .is_err()
        );
    }
    for greeting in [vec![4, 1, 0], vec![5, 0], vec![5, 1, 255], vec![5, 1, 99]] {
        let mut script = Script::new(greeting, 1);
        assert!(blocking::accept(&mut script, &ServerAuth::no_authentication()).is_err());
        assert_eq!(script.written, [5, 255]);
    }
}

#[test]
fn listening_proxy_relays_half_close_and_stops() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let target_addr = target.local_addr().unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_addr = proxy.local_addr().unwrap();
    let shutdown = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let target_worker = scope.spawn(move || {
            let (mut socket, _) = target.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            socket.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"half-close");
            socket.write_all(b"response-after-eof").unwrap();
        });
        let config = ServerConfig::new(ServerAuth::no_authentication(), move |_, _, resolved| {
            resolved == target_addr
        });
        let shutdown_ref = &shutdown;
        let worker =
            scope.spawn(move || blocking::serve(proxy, &config, shutdown_ref, |_| {}).unwrap());
        let (mut tunnel, bound) = blocking::connect_tcp(
            proxy_addr,
            target_addr.into(),
            ClientAuth::NoAuthentication,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_ne!(bound.port(), 0);
        tunnel
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        tunnel.write_all(b"half-close").unwrap();
        tunnel.shutdown(Shutdown::Write).unwrap();
        let mut bytes = Vec::new();
        tunnel.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"response-after-eof");
        target_worker.join().unwrap();
        shutdown.store(true, Ordering::Release);
        worker.join().unwrap();
    });
}

#[test]
fn destination_denial_sends_failure_without_dialing() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let destination = target.local_addr().unwrap();
    target.set_nonblocking(true).unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = proxy.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (socket, _) = proxy.accept().unwrap();
        blocking::serve_connection(
            socket,
            &ServerConfig::new(ServerAuth::no_authentication(), |_, _, _| false),
        )
    });
    assert!(matches!(
        blocking::connect_tcp(
            addr,
            destination.into(),
            ClientAuth::NoAuthentication,
            Duration::from_secs(5)
        ),
        Err(Error::Reply(ReplyCode::ConnectionNotAllowed))
    ));
    assert!(matches!(
        worker.join().unwrap(),
        Err(Error::PermissionDenied)
    ));
    assert_eq!(
        target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn silent_client_cannot_hold_handshake_open() {
    // Recovers the draft's silent-client regression without sleeps or timing assertions.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let mut config = ServerConfig::new(ServerAuth::no_authentication(), |_, _, _| false);
    config.limits.handshake = Duration::from_millis(20);
    let result = blocking::serve_connection(socket, &config);
    assert!(
        matches!(result, Err(Error::Io(error)) if matches!(error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock))
    );
    drop(client);
}

#[test]
fn malformed_credentials_never_reach_the_authentication_callback() {
    let policy =
        ServerAuth::username_password(|_, _| panic!("malformed credentials reached policy"));
    for credentials in [b"\x01\x00\x00".as_slice(), b"\x02\x01u\x01p"] {
        let mut script = Script::new(
            [b"\x05\x01\x02".as_slice(), credentials, REQUEST].concat(),
            1,
        );
        assert!(matches!(
            blocking::accept(&mut script, &policy),
            Err(Error::AuthenticationRejected)
        ));
        assert_eq!(script.written, [5, 2, 1, 1]);
    }
}

#[test]
fn listener_caps_active_handshakes_and_closes_them_on_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let shutdown = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let mut config = ServerConfig::new(ServerAuth::no_authentication(), |_, _, _| false);
        config.limits.connections = 1;
        let stop = &shutdown;
        let worker = scope.spawn(move || blocking::serve(listener, &config, stop, |_| {}));
        let mut first = TcpStream::connect(address).unwrap();
        first
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        first.write_all(&[5, 1, 0]).unwrap();
        let mut selection = [0; 2];
        first.read_exact(&mut selection).unwrap();
        assert_eq!(selection, [5, 0]);
        let mut queued = TcpStream::connect(address).unwrap();
        queued.write_all(&[5, 1, 0]).unwrap();
        // Bounded observation of no service while the only slot is occupied;
        // this is an I/O liveness oracle, not an elapsed-time performance claim.
        queued
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let result = queued.read(&mut selection);
        drop(first);
        queued
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let resumed = queued.read_exact(&mut selection);
        shutdown.store(true, Ordering::Release);
        worker.join().unwrap().unwrap();
        assert!(
            matches!(result, Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
        );
        resumed.unwrap();
        assert_eq!(selection, [5, 0]);
        assert_eq!(queued.read(&mut selection).unwrap(), 0);
    });
}

#[test]
fn relay_deadline_closes_both_open_halves() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let destination = target.local_addr().unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = proxy.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let mut config =
            ServerConfig::new(ServerAuth::no_authentication(), move |_, _, resolved| {
                resolved == destination
            });
        config.limits.relay = Duration::from_millis(20);
        blocking::serve_connection(proxy.accept().unwrap().0, &config)
    });
    let (mut client, _) = blocking::connect_tcp(
        address,
        destination.into(),
        ClientAuth::NoAuthentication,
        Duration::from_secs(5),
    )
    .unwrap();
    let (mut target, _) = target.accept().unwrap();
    let result = worker.join().unwrap();
    assert!(
        matches!(result, Err(Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
    );
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    target
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut byte = [0];
    assert_eq!(client.read(&mut byte).unwrap(), 0);
    assert_eq!(target.read(&mut byte).unwrap(), 0);
}

#[test]
fn listener_shutdown_closes_a_target_that_keeps_its_write_half_open() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let destination = target.local_addr().unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = proxy.local_addr().unwrap();
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let shutdown = std::sync::Arc::clone(&stop);
    let (finished, completion) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let config = ServerConfig::new(ServerAuth::no_authentication(), move |_, _, resolved| {
            resolved == destination
        });
        let result = blocking::serve(proxy, &config, &shutdown, |_| {});
        finished.send(result).unwrap();
    });
    let (mut client, _) = blocking::connect_tcp(
        address,
        destination.into(),
        ClientAuth::NoAuthentication,
        Duration::from_secs(5),
    )
    .unwrap();
    let (mut target, _) = target.accept().unwrap();
    // Keep target alive without writing or closing: client-only shutdown cannot
    // unblock target->client read. The 300-second relay limit must not govern stop.
    stop.store(true, Ordering::Release);
    completion
        .recv_timeout(Duration::from_secs(5))
        .expect("listener shutdown must close both relay endpoints")
        .unwrap();
    worker.join().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    target
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut byte = [0];
    assert_eq!(client.read(&mut byte).unwrap(), 0);
    assert_eq!(target.read(&mut byte).unwrap(), 0);
}
