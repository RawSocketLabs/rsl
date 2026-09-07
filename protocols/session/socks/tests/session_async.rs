//! The same RFC transcripts over async I/O, plus cancellation and TCP relay.

mod support;

use socks::asynchronous as client_server;
use socks::error::Error;
use socks::session::{ClientAuth, ServerAuth, ServerConfig};
use socks::v5::{Endpoint, ReplyCode};
use std::net::SocketAddr;
use std::time::Duration;
use support::{REPLY, REQUEST, Script};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn target() -> Endpoint {
    SocketAddr::from(([127, 0, 0, 1], 1080)).into()
}

#[tokio::test]
async fn ipv6_and_max_domain_frames_preserve_tunnel_data() {
    for (destination, request) in support::variable_address_vectors() {
        let mut reply = request.clone();
        reply[1] = 0;
        let mut script = Script::new([b"\x05\x00".as_slice(), &reply, b"early"].concat(), 1);
        let (tunnel, bound) = client_server::connect(
            &mut script,
            destination.clone(),
            ClientAuth::NoAuthentication,
        )
        .await
        .unwrap();
        assert_eq!(bound, destination);
        let mut early = [0; 5];
        tunnel.read_exact(&mut early).await.unwrap();
        assert_eq!(&early, b"early");
        assert_eq!(
            script.written,
            [b"\x05\x01\x00".as_slice(), &request].concat()
        );
        let mut script = Script::new([b"\x05\x01\x00".as_slice(), &request, b"early"].concat(), 1);
        let pending = client_server::accept(&mut script, &ServerAuth::no_authentication())
            .await
            .unwrap();
        assert_eq!(pending.destination(), &destination);
        pending
            .accept(destination)
            .await
            .unwrap()
            .read_exact(&mut early)
            .await
            .unwrap();
        assert_eq!(&early, b"early");
        assert_eq!(script.written, [b"\x05\x00".as_slice(), &reply].concat());
    }
}

#[tokio::test]
async fn golden_transcripts_and_tunnel_handoff_match_blocking() {
    for chunk in [1, 2, usize::MAX] {
        let mut script = Script::new([b"\x05\x00".as_slice(), REPLY, b"early"].concat(), chunk);
        let (tunnel, bound) =
            client_server::connect(&mut script, target(), ClientAuth::NoAuthentication)
                .await
                .unwrap();
        assert_eq!(bound, target());
        let mut payload = Vec::new();
        tunnel.read_to_end(&mut payload).await.unwrap();
        assert_eq!(payload, b"early");
        assert_eq!(
            script.written,
            [b"\x05\x01\x00".as_slice(), REQUEST].concat()
        );
        let mut script = Script::new(
            [b"\x05\x01\x00".as_slice(), REQUEST, b"early"].concat(),
            chunk,
        );
        let incoming = client_server::accept(&mut script, &ServerAuth::no_authentication())
            .await
            .unwrap();
        assert_eq!(incoming.destination(), &target());
        let tunnel = incoming.accept(target()).await.unwrap();
        let mut payload = Vec::new();
        tunnel.read_to_end(&mut payload).await.unwrap();
        assert_eq!(payload, b"early");
        assert_eq!(script.written, [b"\x05\x00".as_slice(), REPLY].concat());
    }
}

#[tokio::test]
async fn auth_success_rejection_and_downgrade_match_blocking() {
    let auth = ClientAuth::UsernamePassword {
        username: b"u",
        password: b"p",
    };
    let mut script = Script::new([b"\x05\x02\x01\x00".as_slice(), REPLY].concat(), 1);
    client_server::connect(&mut script, target(), auth)
        .await
        .unwrap();
    assert_eq!(
        script.written,
        [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat()
    );
    let policy = ServerAuth::username_password(|u, p| u == b"u" && p == b"p");
    let mut script = Script::new(
        [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat(),
        1,
    );
    client_server::accept(&mut script, &policy)
        .await
        .unwrap()
        .accept(target())
        .await
        .unwrap();
    assert_eq!(
        script.written,
        [b"\x05\x02\x01\x00".as_slice(), REPLY].concat()
    );
    let mut script = Script::new([5, 0], 1);
    assert!(matches!(
        client_server::connect(&mut script, target(), auth).await,
        Err(Error::UnexpectedMethod(_))
    ));
    assert_eq!(script.written, [5, 1, 2]);
    for status in 1..=255 {
        let mut script = Script::new([5, 2, 1, status], 1);
        assert!(matches!(
            client_server::connect(&mut script, target(), auth).await,
            Err(Error::AuthenticationRejected)
        ));
        assert_eq!(script.written, b"\x05\x01\x02\x01\x01u\x01p");
    }
    for credentials in [
        b"\x01\x01u\x01x".as_slice(),
        b"\x01\x00\x00",
        b"\x02\x01u\x01p",
    ] {
        let mut script = Script::new(
            [b"\x05\x01\x02".as_slice(), credentials, REQUEST].concat(),
            1,
        );
        assert!(matches!(
            client_server::accept(&mut script, &policy).await,
            Err(Error::AuthenticationRejected)
        ));
        assert_eq!(script.written, [5, 2, 1, 1]);
        assert_eq!(script.incoming.position(), (3 + credentials.len()) as u64);
    }
}

#[tokio::test]
async fn truncation_and_invalid_sessions_match_blocking() {
    let wire = [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat();
    for end in 0..wire.len() {
        assert!(
            client_server::accept(
                Script::new(wire[..end].to_vec(), 1),
                &ServerAuth::username_password(|_, _| true)
            )
            .await
            .is_err(),
            "prefix {end}"
        );
    }
    let wire = [b"\x05\x02\x01\x00".as_slice(), REPLY].concat();
    for end in 0..wire.len() {
        assert!(
            client_server::connect(
                Script::new(wire[..end].to_vec(), 1),
                target(),
                ClientAuth::UsernamePassword {
                    username: b"u",
                    password: b"p"
                }
            )
            .await
            .is_err(),
            "prefix {end}"
        );
    }
    for (offset, byte, code) in [(0, 4, 1), (1, 2, 7), (1, 3, 7), (2, 1, 1), (3, 99, 8)] {
        let mut request = REQUEST.to_vec();
        request[offset] = byte;
        let mut script = Script::new([b"\x05\x01\x00".as_slice(), &request].concat(), 1);
        assert!(
            client_server::accept(&mut script, &ServerAuth::no_authentication())
                .await
                .is_err()
        );
        assert_eq!(script.written[3], code);
    }
    let mut script = Script::new([b"\x05\x01\x00".as_slice(), REQUEST].concat(), 1);
    client_server::accept(&mut script, &ServerAuth::no_authentication())
        .await
        .unwrap()
        .reject(ReplyCode::ConnectionNotAllowed)
        .await
        .unwrap();
    assert_eq!(script.written, [5, 0, 5, 2, 0, 1, 0, 0, 0, 0, 0, 0]);
}

#[tokio::test]
async fn pending_io_and_cancellation_close_owned_sessions() {
    let (server, mut peer) = tokio::io::duplex(1);
    let policy = ServerAuth::no_authentication();
    let handler = tokio::spawn(async move { client_server::accept(server, &policy).await.is_ok() });
    peer.write_all(&[5]).await.unwrap(); // Partial greeting: read_exact must stay pending.
    handler.abort();
    assert!(handler.await.unwrap_err().is_cancelled());
    let mut byte = [0];
    assert_eq!(peer.read(&mut byte).await.unwrap(), 0);

    let (client, server) = tokio::io::duplex(1);
    let worker = tokio::spawn(async move {
        client_server::accept(server, &ServerAuth::no_authentication())
            .await
            .unwrap()
            .accept(target())
            .await
            .unwrap()
    });
    let (mut client, _) = client_server::connect(client, target(), ClientAuth::NoAuthentication)
        .await
        .unwrap();
    let mut server = worker.await.unwrap();
    let writer = tokio::spawn(async move {
        server.write_all(b"ok").await.unwrap();
    });
    let mut bytes = [0; 2];
    client.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"ok");
    writer.await.unwrap();
}

#[tokio::test]
async fn listening_proxy_authorizes_resolved_domain_and_preserves_half_close() {
    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = target_listener.local_addr().unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy.local_addr().unwrap();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let config = ServerConfig::new(
        ServerAuth::username_password(|u, p| u == b"u" && p == b"p"),
        move |peer, requested, resolved| {
            assert!(peer.ip().is_loopback());
            assert_eq!(
                requested,
                &Endpoint::domain(b"localhost", destination.port())
            );
            resolved == destination
        },
    );
    let proxy_worker = tokio::spawn(async move {
        client_server::serve(
            proxy,
            &config,
            async {
                let _ = shutdown.await;
            },
            |_| {},
        )
        .await
    });
    let echo = tokio::spawn(async move {
        let (mut socket, _) = target_listener.accept().await.unwrap();
        let mut payload = Vec::new();
        socket.read_to_end(&mut payload).await.unwrap();
        assert_eq!(payload, b"half-close");
        socket.write_all(b"after-eof").await.unwrap();
    });
    let (mut tunnel, bound) = client_server::connect_tcp(
        proxy_addr,
        Endpoint::domain(b"localhost", destination.port()),
        ClientAuth::UsernamePassword {
            username: b"u",
            password: b"p",
        },
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_ne!(bound.port(), 0);
    tunnel.write_all(b"half-close").await.unwrap();
    tunnel.shutdown().await.unwrap();
    let mut payload = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), tunnel.read_to_end(&mut payload))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(payload, b"after-eof");
    echo.await.unwrap();
    stop.send(()).unwrap();
    proxy_worker.await.unwrap().unwrap();
}

#[tokio::test]
async fn listener_shutdown_closes_an_active_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        client_server::serve(
            listener,
            &ServerConfig::new(ServerAuth::no_authentication(), |_, _, _| false),
            async {
                let _ = shutdown.await;
            },
            |_| {},
        )
        .await
    });
    let mut peer = TcpStream::connect(addr).await.unwrap();
    peer.write_all(&[5, 1, 0]).await.unwrap();
    let mut selection = [0; 2];
    peer.read_exact(&mut selection).await.unwrap();
    assert_eq!(selection, [5, 0]);
    stop.send(()).unwrap();
    worker.await.unwrap().unwrap();
    let mut byte = [0];
    assert_eq!(peer.read(&mut byte).await.unwrap(), 0);
}

#[tokio::test]
async fn absolute_handshake_deadline_uses_tokio_time() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let _silent = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (server, _) = listener.accept().await.unwrap();
    tokio::time::pause();
    let mut config = ServerConfig::new(ServerAuth::no_authentication(), |_, _, _| false);
    config.limits.handshake = Duration::from_secs(1);
    assert!(
        matches!(client_server::serve_connection(server, &config).await, Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[tokio::test]
async fn malformed_credentials_never_reach_the_authentication_callback() {
    let policy =
        ServerAuth::username_password(|_, _| panic!("malformed credentials reached policy"));
    for credentials in [b"\x01\x00\x00".as_slice(), b"\x02\x01u\x01p"] {
        let mut script = Script::new(
            [b"\x05\x01\x02".as_slice(), credentials, REQUEST].concat(),
            1,
        );
        assert!(matches!(
            client_server::accept(&mut script, &policy).await,
            Err(Error::AuthenticationRejected)
        ));
        assert_eq!(script.written, [5, 2, 1, 1]);
    }
}

#[tokio::test]
async fn listener_capacity_is_released_after_a_session_ends() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        let mut config = ServerConfig::new(ServerAuth::no_authentication(), |_, _, _| false);
        config.limits.connections = 1;
        client_server::serve(
            listener,
            &config,
            async {
                let _ = shutdown.await;
            },
            |_| {},
        )
        .await
    });
    let mut first = TcpStream::connect(address).await.unwrap();
    first.write_all(&[5, 1, 0]).await.unwrap();
    let mut selection = [0; 2];
    first.read_exact(&mut selection).await.unwrap();
    assert_eq!(selection, [5, 0]);
    let mut queued = TcpStream::connect(address).await.unwrap();
    queued.write_all(&[5, 1, 0]).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), queued.read(&mut selection))
            .await
            .is_err()
    );
    drop(first);
    tokio::time::timeout(Duration::from_secs(5), queued.read_exact(&mut selection))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selection, [5, 0]);
    stop.send(()).unwrap();
    worker.await.unwrap().unwrap();
    assert_eq!(queued.read(&mut selection).await.unwrap(), 0);
}

#[tokio::test]
async fn relay_deadline_closes_both_open_halves() {
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = target.local_addr().unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = proxy.local_addr().unwrap();
    let worker = tokio::spawn(async move {
        let mut config =
            ServerConfig::new(ServerAuth::no_authentication(), move |_, _, resolved| {
                resolved == destination
            });
        config.limits.relay = Duration::from_secs(1);
        client_server::serve_connection(proxy.accept().await.unwrap().0, &config).await
    });
    let (mut client, _) = client_server::connect_tcp(
        address,
        destination.into(),
        ClientAuth::NoAuthentication,
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    let (mut target, _) = target.accept().await.unwrap();
    tokio::time::pause();
    assert!(
        matches!(worker.await.unwrap(), Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::TimedOut)
    );
    let mut byte = [0];
    assert_eq!(client.read(&mut byte).await.unwrap(), 0);
    assert_eq!(target.read(&mut byte).await.unwrap(), 0);
}

#[cfg(feature = "blocking")]
#[tokio::test]
async fn blocking_client_interoperates_with_async_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let incoming = client_server::accept(stream, &ServerAuth::no_authentication())
            .await
            .unwrap();
        assert_eq!(incoming.destination(), &target());
        let mut stream = incoming.accept(target()).await.unwrap();
        stream.write_all(b"banner").await.unwrap();
    });
    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = socks::blocking::connect_tcp(
            address,
            target(),
            ClientAuth::NoAuthentication,
            Duration::from_secs(5),
        )
        .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = [0; 6];
        std::io::Read::read_exact(&mut stream, &mut bytes).unwrap();
        assert_eq!(&bytes, b"banner");
    })
    .await
    .unwrap();
    server.await.unwrap();
}
