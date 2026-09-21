//! The same RFC transcripts over async I/O, plus cancellation and TCP relay.

mod support;

use socks::error::Error;
use socks::v5::{Endpoint, ReplyCode};
use socks::{
    proxy::tokio as proxy,
    v5::{client::tokio as client, server::tokio as server},
};
use socks::{server::ServerConfig, server::policy::ServerAuth, v5::auth::ClientAuth};
use std::net::SocketAddr;
use std::time::Duration;
use support::{REPLY, REQUEST, Script};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn target() -> Endpoint {
    SocketAddr::from(([127, 0, 0, 1], 1080)).into()
}

#[tokio::test]
async fn unsupported_greeting_versions_never_authenticate_or_send_a_guessed_reply() {
    let auth =
        ServerAuth::username_password(|_, _| panic!("foreign protocol reached credential policy"));
    for version in (0..=u8::MAX).filter(|version| *version != 5) {
        let mut script = Script::new([version, 1, 2, 1, 1, b'u', 1, b'p'], 1);
        assert!(
            matches!(server::exchange(&mut script, &auth).await, Err(Error::VersionNotAccepted(actual)) if actual == version),
            "version {version}"
        );
        assert!(
            script.written.is_empty(),
            "version {version} must not receive a SOCKS5 reply"
        );
    }
}

#[tokio::test]
async fn configured_client_matches_rfc_transcript_and_retains_coalesced_payload() {
    let client = socks::Client::builder()
        .protocol(socks::v5::client::Config::username_password(b"u", b"p").unwrap())
        .build()
        .unwrap();
    let mut script = Script::new(
        [b"\x05\x02\x01\x00".as_slice(), REPLY, b"payload"].concat(),
        usize::MAX,
    );
    let (mut stream, bound) = client
        .connect_async(&mut script, socks::Destination::from(target()))
        .await
        .unwrap();
    assert_eq!(bound, socks::Destination::from(target()));
    let mut payload = Vec::new();
    stream.read_to_end(&mut payload).await.unwrap();
    assert_eq!(payload, b"payload");
    drop(stream);
    assert_eq!(
        script.written,
        [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat()
    );
    for length in [0, 256] {
        let mut io = Script::new([], 1);
        assert!(matches!(
            client
                .connect_async(&mut io, socks::Destination::domain(vec![b'x'; length], 80))
                .await,
            Err(Error::InvalidEndpoint)
        ));
        assert!(
            io.written.is_empty(),
            "invalid local destination must fail before I/O"
        );
    }
}

async fn queued_request(request: &[u8], payload: &[u8]) -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    client
        .write_all(&[b"\x05\x01\x00", request, payload].concat())
        .await
        .unwrap();
    client.shutdown().await.unwrap();
    (client, listener.accept().await.unwrap().0)
}

#[tokio::test]
async fn server_chain_defers_dial_and_success_and_returns_both_streams() {
    // A nonblocking std listener provides an immediate no-dial observation.
    let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    target.set_nonblocking(true).unwrap();
    let request = [
        b"\x05\x01\x00\x01\x7f\x00\x00\x01".as_slice(),
        &address.port().to_be_bytes(),
    ]
    .concat();
    let (mut client, stream) = queued_request(&request, b"early").await;
    let peer = client.local_addr().unwrap();
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = calls.clone();
    let server = socks::Server::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
            let (actual_peer, requested, resolved) =
                (context.peer, &context.request.destination, context.target);

            assert_eq!(actual_peer, peer);
            assert_eq!(requested, &address.into());
            assert_eq!(resolved, address);
            observed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            true
        }),
    ))
    .unwrap();
    let exchange = server.exchange_async(stream).await.unwrap();
    assert_eq!(exchange.destination(), &address.into());
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 0);
    let mut selection = [0; 2];
    client.read_exact(&mut selection).await.unwrap();
    assert_eq!(selection, [5, 0]);
    let authorized = exchange.authorize().await.unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(
        target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        client.try_read(&mut [0]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let connection = authorized.connect().await.unwrap();
    let (destination, bound) = target.accept().unwrap();
    destination.set_nonblocking(true).unwrap();
    let mut destination = TcpStream::from_std(destination).unwrap();
    let (mut incoming, mut outgoing) = connection.into_parts();
    assert_eq!(outgoing.peer_addr().unwrap(), address);
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    let mut reply = [0; 10];
    client.read_exact(&mut reply).await.unwrap();
    assert_eq!(&reply[..8], b"\x05\x00\x00\x01\x7f\x00\x00\x01");
    assert_eq!(&reply[8..], &bound.port().to_be_bytes());
    let mut early = Vec::new();
    incoming.read_to_end(&mut early).await.unwrap();
    assert_eq!(early, b"early");
    outgoing.write_all(b"live").await.unwrap();
    let mut bytes = [0; 4];
    destination.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"live");
}

#[tokio::test]
async fn server_manual_handoff_preserves_input_without_running_destination_policy() {
    let (mut client, stream) = queued_request(REQUEST, b"early").await;
    let server = socks::Server::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            panic!("manual path must not authorize")
        }),
    ))
    .unwrap();
    let request = server.exchange_async(stream).await.unwrap().into_request();
    assert_eq!(request.destination(), &target());
    let mut stream = request.send_success(target()).await.unwrap();
    let mut early = Vec::new();
    stream.read_to_end(&mut early).await.unwrap();
    assert_eq!(early, b"early");
    drop(stream);
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).await.unwrap();
    assert_eq!(reply, [b"\x05\x00".as_slice(), REPLY].concat());
}

#[tokio::test]
async fn server_resolution_failure_sends_host_unreachable_without_policy_or_dial() {
    let (mut client, stream) = queued_request(b"\x05\x01\x00\x03\x01\xff\x00\x50", b"").await;
    let server = socks::Server::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            panic!("invalid domain reached policy")
        }),
    ))
    .unwrap();
    assert!(matches!(
        server
            .exchange_async(stream)
            .await
            .unwrap()
            .authorize()
            .await,
        Err(Error::InvalidEndpoint)
    ));
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).await.unwrap();
    assert_eq!(reply, [5, 0, 5, 4, 0, 1, 0, 0, 0, 0, 0, 0]);
}

#[tokio::test]
async fn connect_rejects_zero_ports_before_client_io_or_server_handoff() {
    let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    for wire in support::zero_port_requests() {
        let destination = socks::v5::Request::decode_exact(&wire).unwrap().destination;
        let mut script = Script::new([], 1);
        assert!(matches!(
            client::connect(
                &mut script,
                destination.clone(),
                ClientAuth::NoAuthentication
            )
            .await,
            Err(Error::ZeroDestinationPort)
        ));
        assert_eq!(script.read_calls, 0);
        assert!(script.written.is_empty());
        assert!(matches!(
            client::connect_tcp(
                proxy.local_addr().unwrap(),
                destination,
                ClientAuth::NoAuthentication,
                Duration::from_secs(1)
            )
            .await,
            Err(Error::ZeroDestinationPort)
        ));
        assert!(
            matches!(proxy.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );

        let mut script = Script::new([b"\x05\x01\x00".as_slice(), &wire].concat(), 1);
        assert!(matches!(
            server::exchange(&mut script, &ServerAuth::no_authentication()).await,
            Err(Error::ZeroDestinationPort)
        ));
        assert_eq!(script.written, [5, 0, 5, 1, 0, 1, 0, 0, 0, 0, 0, 0]);
    }
}

#[tokio::test]
async fn server_connect_failure_sends_refusal_without_success() {
    // Reserve a nonzero TCP port without listening, avoiding a bind/drop race.
    let target = tokio::net::TcpSocket::new_v4().unwrap();
    target.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let request = [
        b"\x05\x01\x00\x01\x7f\x00\x00\x01".as_slice(),
        &target.local_addr().unwrap().port().to_be_bytes(),
    ]
    .concat();
    let (mut client, stream) = queued_request(&request, b"").await;
    let server = socks::Server::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            true
        }),
    ))
    .unwrap();
    let result = server
        .exchange_async(stream)
        .await
        .unwrap()
        .authorize()
        .await
        .unwrap()
        .connect()
        .await;
    assert!(
        matches!(result, Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused)
    );
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).await.unwrap();
    assert_eq!(reply, [5, 0, 5, 5, 0, 1, 0, 0, 0, 0, 0, 0]);
}

#[tokio::test]
async fn server_authorization_denial_sends_failure_without_dialing() {
    let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    target.set_nonblocking(true).unwrap();
    let request = [
        b"\x05\x01\x00\x01\x7f\x00\x00\x01".as_slice(),
        &address.port().to_be_bytes(),
    ]
    .concat();
    let (mut client, stream) = queued_request(&request, b"").await;
    let server = socks::Server::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
            let (_, _, resolved) = (context.peer, &context.request.destination, context.target);

            assert_eq!(resolved, address);
            false
        }),
    ))
    .unwrap();
    assert!(matches!(
        server
            .exchange_async(stream)
            .await
            .unwrap()
            .authorize()
            .await,
        Err(Error::PermissionDenied)
    ));
    assert_eq!(
        target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).await.unwrap();
    assert_eq!(reply, [5, 0, 5, 2, 0, 1, 0, 0, 0, 0, 0, 0]);
}

#[tokio::test]
async fn dropping_server_stages_closes_client_without_dial_or_success() {
    let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    target.set_nonblocking(true).unwrap();
    let request = [
        b"\x05\x01\x00\x01\x7f\x00\x00\x01".as_slice(),
        &address.port().to_be_bytes(),
    ]
    .concat();
    let server = socks::Server::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            true
        }),
    ))
    .unwrap();
    for authorize in [false, true] {
        let (mut client, stream) = queued_request(&request, b"").await;
        let exchange = server.exchange_async(stream).await.unwrap();
        if authorize {
            drop(exchange.authorize().await.unwrap());
        } else {
            drop(exchange);
        }
        assert_eq!(
            target.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        let mut reply = Vec::new();
        client.read_to_end(&mut reply).await.unwrap();
        assert_eq!(
            reply,
            [5, 0],
            "dropping authorize={authorize} must not send success"
        );
    }
}

#[tokio::test]
async fn server_connect_keeps_authorization_deadline_and_uses_a_fresh_reply_budget() {
    let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    target.set_nonblocking(true).unwrap();
    let request = [
        b"\x05\x01\x00\x01\x7f\x00\x00\x01".as_slice(),
        &address.port().to_be_bytes(),
    ]
    .concat();
    let (mut client, stream) = queued_request(&request, b"").await;
    let mut config = ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            true
        }),
    );
    config.limits.connect = Duration::from_secs(1);
    let server = socks::Server::new(config).unwrap();
    let exchange = server.exchange_async(stream).await.unwrap();
    tokio::time::pause();
    let authorized = exchange.authorize().await.unwrap();
    tokio::time::advance(Duration::from_secs(2)).await;
    assert!(
        matches!(authorized.connect().await, Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    assert_eq!(
        target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).await.unwrap();
    assert_eq!(reply, [5, 0, 5, 1, 0, 1, 0, 0, 0, 0, 0, 0]);
}

#[tokio::test]
async fn server_exchange_cancellation_closes_the_owned_transport() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (stream, _) = listener.accept().await.unwrap();
    let server = socks::Server::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            panic!("cancelled exchange reached policy")
        }),
    ))
    .unwrap();
    let mut exchange = Box::pin(server.exchange_async(stream));
    let result = std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(exchange.as_mut(), cx))
    })
    .await;
    assert!(result.is_pending());
    drop(exchange);
    assert_eq!(client.read(&mut [0]).await.unwrap(), 0);
}

#[tokio::test]
async fn server_policy_deadline_takes_precedence_over_both_allow_and_deny() {
    tokio::time::pause();
    for permit in [false, true] {
        let (mut client, stream) = queued_request(REQUEST, b"").await;
        let runtime = tokio::runtime::Handle::current();
        let mut config = ServerConfig::new(
            [socks::Version::V5],
            socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
                let (_, _, _) = (context.peer, &context.request.destination, context.target);

                let runtime = runtime.clone();
                // Advance the fake clock during this synchronous callback. Enter the
                // runtime from a helper thread instead of nesting block_on in a worker.
                std::thread::spawn(move || {
                    runtime.block_on(tokio::time::advance(Duration::from_secs(2)));
                })
                .join()
                .unwrap();
                permit
            }),
        );
        config.limits.connect = Duration::from_secs(1);
        let server = socks::Server::new(config).unwrap();
        let result = server
            .exchange_async(stream)
            .await
            .unwrap()
            .authorize()
            .await;
        assert!(
            matches!(result, Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut),
            "policy expiry must win even when permit={permit}"
        );
        let mut reply = Vec::new();
        client.read_to_end(&mut reply).await.unwrap();
        assert_eq!(reply, [5, 0, 5, 1, 0, 1, 0, 0, 0, 0, 0, 0]);
    }
}

#[tokio::test]
async fn handoff_preserves_prefix_without_polling_a_pending_transport() {
    let input = [b"\x05\x00".as_slice(), REPLY, b"early"].concat();
    let (tunnel, _) = client::connect(
        Script::new(input, usize::MAX),
        target(),
        ClientAuth::NoAuthentication,
    )
    .await
    .unwrap();
    assert_eq!(
        tunnel.get_ref().read_calls,
        1,
        "coalesced input needs one read"
    );
    let tunnel = tunnel
        .try_into_inner()
        .err()
        .expect("prefix must be retained");
    let (mut transport, buffer) = tunnel.into_parts();
    transport.read_pending = true;
    let mut tunnel = socks::Stream::from_parts(transport, buffer);
    let mut bytes = [0; 32];
    // Poll once: waiting for inner I/O after producing the prefix is a regression.
    let mut output = tokio::io::ReadBuf::new(&mut bytes);
    let result = std::future::poll_fn(|cx| {
        std::task::Poll::Ready(tokio::io::AsyncRead::poll_read(
            std::pin::Pin::new(&mut tunnel),
            cx,
            &mut output,
        ))
    })
    .await;
    assert!(matches!(result, std::task::Poll::Ready(Ok(()))));
    assert_eq!(output.filled(), b"early");
    assert_eq!(tunnel.read(&mut []).await.unwrap(), 0);
    let mut output = tokio::io::ReadBuf::new(&mut bytes);
    let result = std::future::poll_fn(|cx| {
        std::task::Poll::Ready(tokio::io::AsyncRead::poll_read(
            std::pin::Pin::new(&mut tunnel),
            cx,
            &mut output,
        ))
    })
    .await;
    assert!(
        result.is_pending(),
        "once drained, transport readiness governs reads"
    );
    assert!(output.filled().is_empty());
    tunnel.get_mut().read_pending = false;
    tunnel.get_mut().read_error = Some(std::io::ErrorKind::ConnectionReset);
    let error = tunnel.read(&mut bytes).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
    assert!(error.get_ref().is_some());
    tunnel.write_all(b"outgoing").await.unwrap();
    tunnel.flush().await.unwrap();
    tunnel.get_mut().flush_error = Some(std::io::ErrorKind::BrokenPipe);
    assert_eq!(
        tunnel.flush().await.unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    tunnel.shutdown().await.unwrap();
    assert!(tunnel.get_ref().written.ends_with(b"outgoing"));
    assert!(tunnel.try_into_inner().is_ok());
}

#[tokio::test]
async fn handoff_rejects_partial_bytes_without_consuming_them() {
    use bnb::Source;
    let mut buffer = bnb::BitBuf::new();
    buffer.push(&[0xab, 0xcd]).unwrap();
    buffer.read_bits(4).unwrap();
    let mut tunnel = socks::Stream::from_parts(Script::new([], 1), buffer);
    assert_eq!(tunnel.read(&mut []).await.unwrap(), 0);
    let error = tunnel.read(&mut [0]).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    let (transport, mut buffer) = tunnel.into_parts();
    assert_eq!(transport.read_calls, 0);
    assert_eq!(buffer.read_bits(12).unwrap(), 0xbcd);
}

#[tokio::test]
async fn incremental_errors_distinguish_empty_eof_truncation_and_transport_failure() {
    let mut script = Script::new([b"\x05\x00".as_slice(), REPLY].concat(), 1);
    script.read_error = Some(std::io::ErrorKind::Interrupted);
    assert!(
        client::connect(script, target(), ClientAuth::NoAuthentication)
            .await
            .is_ok()
    );
    for (input, expected_codec) in [(vec![], false), (vec![5], true)] {
        let error = client::connect(
            Script::new(input, usize::MAX),
            target(),
            ClientAuth::NoAuthentication,
        )
        .await
        .err()
        .unwrap();
        match error {
            Error::Codec(error) if expected_codec => {
                assert!(matches!(error.kind, bnb::ErrorKind::UnexpectedEof { .. }));
            }
            Error::Io(error) if !expected_codec => {
                assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
            }
            error => panic!("unexpected EOF classification: {error}"),
        }
    }
    let mut script = Script::new([], 1);
    script.read_error = Some(std::io::ErrorKind::ConnectionReset);
    let error = client::connect(script, target(), ClientAuth::NoAuthentication)
        .await
        .err()
        .unwrap();
    assert!(matches!(error, Error::Io(error) if error.get_ref().is_some()));
}

#[tokio::test]
async fn unknown_address_headers_keep_typed_errors_and_rfc_failure_replies() {
    for code in (0..=u8::MAX).filter(|code| ![1, 3, 4].contains(code)) {
        for chunk in [1, usize::MAX] {
            let mut script = Script::new([5, 1, 0, 5, 1, 0, code], chunk);
            assert!(
                matches!(server::exchange(&mut script, &ServerAuth::no_authentication()).await,
                Err(Error::UnsupportedAddressType(actual)) if actual == code),
                "ATYP {code}, chunk {chunk}"
            );
            assert_eq!(script.written, [5, 0, 5, 8, 0, 1, 0, 0, 0, 0, 0, 0]);
            assert!(
                matches!(client::connect(Script::new([5, 0, 5, 0, 0, code], chunk), target(), ClientAuth::NoAuthentication).await,
                Err(Error::UnsupportedAddressType(actual)) if actual == code),
                "ATYP {code}, chunk {chunk}"
            );
        }
    }
    for code in [1, 3, 4] {
        let error = client::connect(
            Script::new([5, 0, 5, 0, 0, code], usize::MAX),
            target(),
            ClientAuth::NoAuthentication,
        )
        .await
        .err()
        .unwrap();
        assert!(
            matches!(error, Error::Codec(error) if matches!(error.kind, bnb::ErrorKind::UnexpectedEof { .. })),
            "ATYP {code} has a truncated payload"
        );
    }
}

#[tokio::test]
async fn ipv6_and_max_domain_frames_preserve_tunnel_data() {
    for (destination, request) in support::variable_address_vectors() {
        let mut reply = request.clone();
        reply[1] = 0;
        let mut script = Script::new([b"\x05\x00".as_slice(), &reply, b"early"].concat(), 1);
        let (mut tunnel, bound) = client::connect(
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
        let request = server::exchange(&mut script, &ServerAuth::no_authentication())
            .await
            .unwrap();
        assert_eq!(request.destination(), &destination);
        request
            .send_success(destination)
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
        let (mut tunnel, bound) =
            client::connect(&mut script, target(), ClientAuth::NoAuthentication)
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
        let incoming = server::exchange(&mut script, &ServerAuth::no_authentication())
            .await
            .unwrap();
        assert_eq!(incoming.destination(), &target());
        let mut tunnel = incoming.send_success(target()).await.unwrap();
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
    client::connect(&mut script, target(), auth).await.unwrap();
    assert_eq!(
        script.written,
        [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat()
    );
    let policy = ServerAuth::username_password(|u, p| u == b"u" && p == b"p");
    let mut script = Script::new(
        [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat(),
        1,
    );
    server::exchange(&mut script, &policy)
        .await
        .unwrap()
        .send_success(target())
        .await
        .unwrap();
    assert_eq!(
        script.written,
        [b"\x05\x02\x01\x00".as_slice(), REPLY].concat()
    );
    let mut script = Script::new([5, 0], 1);
    assert!(matches!(
        client::connect(&mut script, target(), auth).await,
        Err(Error::UnexpectedMethod(_))
    ));
    assert_eq!(script.written, [5, 1, 2]);
    for status in 1..=255 {
        let mut script = Script::new([5, 2, 1, status], 1);
        assert!(matches!(
            client::connect(&mut script, target(), auth).await,
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
            server::exchange(&mut script, &policy).await,
            Err(Error::AuthenticationRejected)
        ));
        assert_eq!(script.written, [5, 2, 1, 1]);
        assert_eq!(script.incoming.position(), (3 + credentials.len()) as u64);
    }
}

#[tokio::test]
async fn maximum_credentials_complete_server_authentication() {
    for chunk in [1, 2, 257, 513, usize::MAX] {
        let mut script = Script::new(support::maximum_credentials_transcript(), chunk);
        let policy = ServerAuth::username_password(|user, password| {
            user == [7; 255] && password == [8; 255]
        });

        server::exchange(&mut script, &policy).await.unwrap();

        assert!(
            script.max_read_len <= 513,
            "bounded handshake reads, chunk {chunk}"
        );
        assert_eq!(script.written, [5, 2, 1, 0], "chunk {chunk}");
    }
}

#[tokio::test]
async fn shortfall_wait_finishes_truncated_variable_fields_at_eof() {
    let input = support::maximum_credentials_transcript();
    let policy =
        ServerAuth::username_password(|_, _| panic!("truncated credentials reached policy"));
    for chunk in [1, 16, usize::MAX] {
        for end in [5, 20, 260, 280, 515] {
            let mut script = Script::new(input[..end].to_vec(), chunk);
            let result = server::exchange(&mut script, &policy).await;
            assert!(
                matches!(result, Err(Error::Codec(error)) if matches!(error.kind, bnb::ErrorKind::UnexpectedEof { .. })),
                "credential prefix {end}, chunk {chunk}"
            );
            assert_eq!(script.written, [5, 2]);
        }
        for (endpoint, mut reply) in support::variable_address_vectors() {
            reply[1] = 0;
            for end in [5, reply.len() - 2, reply.len() - 1] {
                let script = Script::new([b"\x05\x00".as_slice(), &reply[..end]].concat(), chunk);
                let result =
                    client::connect(script, endpoint.clone(), ClientAuth::NoAuthentication).await;
                assert!(
                    matches!(result, Err(Error::Codec(error)) if matches!(error.kind, bnb::ErrorKind::UnexpectedEof { .. })),
                    "reply {endpoint:?}, prefix {end}, chunk {chunk}"
                );
            }
        }
    }
}

#[tokio::test]
async fn shortfall_wait_retries_interruptions_and_preserves_transport_errors() {
    for offset in [20, 280] {
        for kind in [
            std::io::ErrorKind::Interrupted,
            std::io::ErrorKind::ConnectionReset,
        ] {
            let input = [support::maximum_credentials_transcript(), b"tail".to_vec()].concat();
            let mut script = Script::new(input, 1);
            script.read_error_at = Some((offset, kind));
            let policy =
                ServerAuth::username_password(|user, pass| user == [7; 255] && pass == [8; 255]);
            let result = server::exchange(&mut script, &policy).await;
            if kind == std::io::ErrorKind::Interrupted {
                let mut tunnel = result.unwrap().send_success(target()).await.unwrap();
                let mut tail = [0; 4];
                tunnel.read_exact(&mut tail).await.unwrap();
                assert_eq!(&tail, b"tail", "interrupted at {offset}");
            } else {
                assert!(
                    matches!(result, Err(Error::Io(error)) if error.kind() == kind && error.get_ref().is_some()),
                    "I/O error at {offset}"
                );
            }
        }
    }
}

#[tokio::test]
async fn truncation_and_invalid_sessions_match_blocking() {
    let wire = [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat();
    for end in 0..wire.len() {
        assert!(
            server::exchange(
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
            client::connect(
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
            server::exchange(&mut script, &ServerAuth::no_authentication())
                .await
                .is_err()
        );
        assert_eq!(script.written[3], code);
    }
    let mut script = Script::new([b"\x05\x01\x00".as_slice(), REQUEST].concat(), 1);
    server::exchange(&mut script, &ServerAuth::no_authentication())
        .await
        .unwrap()
        .send_failure(ReplyCode::ConnectionNotAllowed)
        .await
        .unwrap();
    assert_eq!(script.written, [5, 0, 5, 2, 0, 1, 0, 0, 0, 0, 0, 0]);
}

#[tokio::test]
async fn shortfall_wait_resumes_after_pending_io() {
    let input = [support::maximum_credentials_transcript(), b"tail".to_vec()].concat();
    for split in [20, 280] {
        let (server, mut peer) = tokio::io::duplex(input.len());
        let policy =
            ServerAuth::username_password(|user, pass| user == [7; 255] && pass == [8; 255]);
        let mut handshake = Box::pin(server::exchange(server, &policy));
        peer.write_all(&input[..split]).await.unwrap();
        let result = std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(handshake.as_mut(), cx))
        })
        .await;
        assert!(result.is_pending(), "partial credentials at {split}");
        peer.write_all(&input[split..]).await.unwrap();
        let mut tunnel = handshake
            .await
            .unwrap()
            .send_success(target())
            .await
            .unwrap();
        let mut tail = [0; 4];
        tunnel.read_exact(&mut tail).await.unwrap();
        assert_eq!(&tail, b"tail", "resumed at {split}");
    }
}

#[tokio::test]
async fn pending_io_and_cancellation_close_owned_sessions() {
    let (server, mut peer) = tokio::io::duplex(1);
    let policy = ServerAuth::no_authentication();
    let handler = tokio::spawn(async move { server::exchange(server, &policy).await.is_ok() });
    peer.write_all(&[5]).await.unwrap(); // Partial greeting: incremental read stays pending.
    handler.abort();
    assert!(handler.await.unwrap_err().is_cancelled());
    let mut byte = [0];
    assert_eq!(peer.read(&mut byte).await.unwrap(), 0);

    let (client, server) = tokio::io::duplex(1);
    let worker = tokio::spawn(async move {
        server::exchange(server, &ServerAuth::no_authentication())
            .await
            .unwrap()
            .send_success(target())
            .await
            .unwrap()
    });
    let (mut client, _) = client::connect(client, target(), ClientAuth::NoAuthentication)
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
async fn pipelined_payload_and_fin_survive_full_proxy_handoff() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_addr = target.local_addr().unwrap();
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = TcpStream::connect(proxy.local_addr().unwrap())
            .await
            .unwrap();
        let payload: Vec<u8> = (0..=255).cycle().take(8192).collect();
        let input = [
            b"\x05\x01\x00\x05\x01\x00\x01\x7f\x00\x00\x01".as_slice(),
            &target_addr.port().to_be_bytes(),
            &payload,
        ]
        .concat();
        client.write_all(&input).await.unwrap();
        client.shutdown().await.unwrap();
        let destination = async {
            let (mut stream, _) = target.accept().await.unwrap();
            let mut received = Vec::new();
            stream.read_to_end(&mut received).await.unwrap();
            assert_eq!(
                received, payload,
                "deliver prefetched and unread bytes once, in order"
            );
            stream.write_all(b"response after FIN").await.unwrap();
        };
        let relay = async {
            let config = ServerConfig::new(
                [socks::Version::V5],
                socks::server::policy::Policy::new(
                    ServerAuth::no_authentication(),
                    move |context| {
                        let (_, _, resolved) =
                            (context.peer, &context.request.destination, context.target);

                        resolved == target_addr
                    },
                ),
            );
            proxy::serve_connection(
                proxy.accept().await.unwrap().0,
                &socks::Server::new(config).unwrap(),
            )
            .await
            .unwrap();
        };
        let response = async {
            let mut replies = [0; 12];
            client.read_exact(&mut replies).await.unwrap();
            assert_eq!(&replies[..6], &[5, 0, 5, 0, 0, 1]);
            let mut response = Vec::new();
            client.read_to_end(&mut response).await.unwrap();
            assert_eq!(response, b"response after FIN");
        };
        tokio::join!(destination, relay, response);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn listening_proxy_authorizes_resolved_domain_and_preserves_half_close() {
    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = target_listener.local_addr().unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy.local_addr().unwrap();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let server = socks::Server::builder()
        .protocols([socks::Version::V5])
        .policy(socks::server::policy::Policy::new(
            ServerAuth::username_password(|u, p| u == b"u" && p == b"p"),
            move |context| {
                let (peer, requested, resolved) =
                    (context.peer, &context.request.destination, context.target);

                assert!(peer.ip().is_loopback());
                assert_eq!(
                    requested,
                    &socks::Destination::domain(b"localhost", destination.port())
                );
                resolved == destination
            },
        ))
        .build()
        .unwrap();
    let proxy_worker = tokio::spawn(async move {
        proxy::serve(
            proxy,
            &server,
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
    let (mut tunnel, bound) = client::connect_tcp(
        proxy_addr,
        Endpoint::domain(b"localhost", destination.port()).unwrap(),
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
        proxy::serve(
            listener,
            &socks::Server::new(ServerConfig::new(
                [socks::Version::V5],
                socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
                    let (_, _, _) = (context.peer, &context.request.destination, context.target);
                    false
                }),
            ))
            .unwrap(),
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
    let mut config = ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            false
        }),
    );
    config.limits.handshake = Duration::from_secs(1);
    assert!(
        matches!(proxy::serve_connection(server, &socks::Server::new(config).unwrap()).await, Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut)
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
            server::exchange(&mut script, &policy).await,
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
        let mut config = ServerConfig::new(
            [socks::Version::V5],
            socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
                let (_, _, _) = (context.peer, &context.request.destination, context.target);
                false
            }),
        );
        config.limits.connections = 1;
        proxy::serve(
            listener,
            &socks::Server::new(config).unwrap(),
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
        let mut config = ServerConfig::new(
            [socks::Version::V5],
            socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
                let (_, _, resolved) = (context.peer, &context.request.destination, context.target);

                resolved == destination
            }),
        );
        config.limits.relay = Duration::from_secs(1);
        proxy::serve_connection(
            proxy.accept().await.unwrap().0,
            &socks::Server::new(config).unwrap(),
        )
        .await
    });
    let (mut client, _) = client::connect_tcp(
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
        let incoming = server::exchange(stream, &ServerAuth::no_authentication())
            .await
            .unwrap();
        assert_eq!(incoming.destination(), &target());
        let mut stream = incoming.send_success(target()).await.unwrap();
        stream.write_all(b"banner").await.unwrap();
    });
    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = socks::v5::client::blocking::connect_tcp(
            address,
            target(),
            ClientAuth::NoAuthentication,
            Duration::from_secs(5),
        )
        .unwrap();
        stream
            .get_ref()
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
