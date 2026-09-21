//! Protocol-derived transcript tests, with fragmented/coalesced I/O and loopback relay.

mod support;

use socks::error::Error;
use socks::v5::{Endpoint, ReplyCode};
use socks::{
    proxy::blocking as proxy,
    v5::{client::blocking as client, server::blocking as server},
};
use socks::{server::ServerConfig, server::policy::ServerAuth, v5::auth::ClientAuth};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use support::{REPLY, REQUEST, Script};

fn target() -> Endpoint {
    SocketAddr::from(([127, 0, 0, 1], 1080)).into()
}

#[test]
fn unsupported_greeting_versions_never_authenticate_or_send_a_guessed_reply() {
    let auth =
        ServerAuth::username_password(|_, _| panic!("foreign protocol reached credential policy"));
    for version in (0..=u8::MAX).filter(|version| *version != 5) {
        let mut script = Script::new([version, 1, 2, 1, 1, b'u', 1, b'p'], 1);
        assert!(
            matches!(server::exchange(&mut script, &auth), Err(Error::VersionNotAccepted(actual)) if actual == version),
            "version {version}"
        );
        assert!(
            script.written.is_empty(),
            "version {version} must not receive a SOCKS5 reply"
        );
    }
}

#[test]
fn configured_client_matches_rfc_transcript_and_retains_coalesced_payload() {
    let client = socks::Client::builder()
        .protocol(socks::v5::client::Config::username_password(b"u", b"p").unwrap())
        .build()
        .unwrap();
    let mut script = Script::new(
        [b"\x05\x02\x01\x00".as_slice(), REPLY, b"payload"].concat(),
        usize::MAX,
    );
    let (mut stream, bound) = client
        .connect(&mut script, socks::Destination::from(target()))
        .unwrap();
    assert_eq!(bound, socks::Destination::from(target()));
    let mut payload = Vec::new();
    stream.read_to_end(&mut payload).unwrap();
    assert_eq!(payload, b"payload");
    drop(stream);
    assert_eq!(
        script.written,
        [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat()
    );
    for length in [0, 256] {
        let mut io = Script::new([], 1);
        assert!(matches!(
            client.connect(&mut io, socks::Destination::domain(vec![b'x'; length], 80)),
            Err(Error::InvalidEndpoint)
        ));
        assert!(
            io.written.is_empty(),
            "invalid local destination must fail before I/O"
        );
    }
}

// Queue an independent RFC 1928 transcript before the server reads, including
// optional early application data. Only ephemeral loopback sockets are used.
fn queued_request(request: &[u8], payload: &[u8]) -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    client
        .write_all(&[b"\x05\x01\x00", request, payload].concat())
        .unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    (client, listener.accept().unwrap().0)
}

#[test]
fn server_chain_defers_dial_and_success_and_returns_both_streams() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    target.set_nonblocking(true).unwrap();
    let request = [
        b"\x05\x01\x00\x01\x7f\x00\x00\x01".as_slice(),
        &address.port().to_be_bytes(),
    ]
    .concat();
    let (mut client, stream) = queued_request(&request, b"early");
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
            observed.fetch_add(1, Ordering::Relaxed);
            true
        }),
    ))
    .unwrap();
    let exchange = server.exchange(stream).unwrap();
    assert_eq!(exchange.destination(), &address.into());
    assert_eq!(
        calls.load(Ordering::Relaxed),
        0,
        "exchange must not authorize"
    );
    let mut selection = [0; 2];
    client.read_exact(&mut selection).unwrap();
    assert_eq!(selection, [5, 0]);
    let authorized = exchange.authorize().unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    client.set_nonblocking(true).unwrap();
    assert_eq!(
        client.peek(&mut [0]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "authorization must not send CONNECT success"
    );
    client.set_nonblocking(false).unwrap();

    let connection = authorized.connect().unwrap();
    let (mut destination, bound) = target.accept().unwrap();
    let (mut incoming, mut outgoing) = connection.into_parts();
    assert_eq!(outgoing.peer_addr().unwrap(), address);
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "connect must use the authorization snapshot"
    );
    assert_eq!(incoming.get_ref().read_timeout().unwrap(), None);
    assert_eq!(incoming.get_ref().write_timeout().unwrap(), None);
    let mut reply = [0; 10];
    client.read_exact(&mut reply).unwrap();
    assert_eq!(&reply[..8], b"\x05\x00\x00\x01\x7f\x00\x00\x01");
    assert_eq!(&reply[8..], &bound.port().to_be_bytes());
    let mut early = Vec::new();
    incoming.read_to_end(&mut early).unwrap();
    assert_eq!(early, b"early");
    outgoing.write_all(b"live").unwrap();
    destination
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = [0; 4];
    destination.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"live");
}

#[test]
fn server_manual_handoff_clears_deadlines_without_running_destination_policy() {
    let (mut client, stream) = queued_request(REQUEST, b"early");
    let server = socks::Server::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            panic!("manual path must not authorize")
        }),
    ))
    .unwrap();
    let request = server.exchange(stream).unwrap().into_request();
    assert_eq!(request.destination(), &target());
    let mut stream = request.send_success(target()).unwrap();
    assert_eq!(stream.get_ref().read_timeout().unwrap(), None);
    assert_eq!(stream.get_ref().write_timeout().unwrap(), None);
    let mut early = Vec::new();
    stream.read_to_end(&mut early).unwrap();
    assert_eq!(early, b"early");
    drop(stream);
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).unwrap();
    assert_eq!(reply, [b"\x05\x00".as_slice(), REPLY].concat());
}

#[test]
fn server_resolution_failure_sends_host_unreachable_without_policy_or_dial() {
    // Opaque domain bytes decode, but cannot enter the system UTF-8 resolver.
    let (mut client, stream) = queued_request(b"\x05\x01\x00\x03\x01\xff\x00\x50", b"");
    let server = socks::Server::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            panic!("invalid domain reached policy")
        }),
    ))
    .unwrap();
    assert!(matches!(
        server.exchange(stream).unwrap().authorize(),
        Err(Error::InvalidEndpoint)
    ));
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).unwrap();
    assert_eq!(reply, [5, 0, 5, 4, 0, 1, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn connect_rejects_zero_ports_before_client_io_or_server_handoff() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    for wire in support::zero_port_requests() {
        let destination = socks::v5::Request::decode_exact(&wire).unwrap().destination;
        let mut script = Script::new([], 1);
        assert!(matches!(
            client::connect(
                &mut script,
                destination.clone(),
                ClientAuth::NoAuthentication
            ),
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
            ),
            Err(Error::ZeroDestinationPort)
        ));
        assert!(
            matches!(proxy.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );

        let mut script = Script::new([b"\x05\x01\x00".as_slice(), &wire].concat(), 1);
        assert!(matches!(
            server::exchange(&mut script, &ServerAuth::no_authentication()),
            Err(Error::ZeroDestinationPort)
        ));
        assert_eq!(script.written, [5, 0, 5, 1, 0, 1, 0, 0, 0, 0, 0, 0]);
    }
}

#[test]
fn client_accepts_zero_bound_port_in_success_reply() {
    // The nonzero CONNECT destination must not constrain the proxy's bound endpoint.
    let response = [5, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    let mut script = Script::new(
        [b"\x05\x00".as_slice(), &response, b"payload"].concat(),
        usize::MAX,
    );
    let (mut stream, bound) =
        client::connect(&mut script, target(), ClientAuth::NoAuthentication).unwrap();
    assert_eq!(bound, SocketAddr::from(([0, 0, 0, 0], 0)).into());
    let mut payload = Vec::new();
    stream.read_to_end(&mut payload).unwrap();
    assert_eq!(payload, b"payload");
    drop(stream);
    assert_eq!(
        script.written,
        [b"\x05\x01\x00".as_slice(), REQUEST].concat()
    );
}

#[test]
fn server_connect_failure_sends_refusal_without_success() {
    // Reserve a nonzero TCP port without listening, avoiding a bind/drop race.
    let target = tokio::net::TcpSocket::new_v4().unwrap();
    target.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let request = [
        b"\x05\x01\x00\x01\x7f\x00\x00\x01".as_slice(),
        &target.local_addr().unwrap().port().to_be_bytes(),
    ]
    .concat();
    let (mut client, stream) = queued_request(&request, b"");
    let server = socks::Server::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            true
        }),
    ))
    .unwrap();
    let result = server
        .exchange(stream)
        .unwrap()
        .authorize()
        .unwrap()
        .connect();
    assert!(
        matches!(result, Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused)
    );
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).unwrap();
    assert_eq!(reply, [5, 0, 5, 5, 0, 1, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn dropping_server_stages_closes_client_without_dial_or_success() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
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
        let (mut client, stream) = queued_request(&request, b"");
        let exchange = server.exchange(stream).unwrap();
        if authorize {
            drop(exchange.authorize().unwrap());
        } else {
            drop(exchange);
        }
        assert_eq!(
            target.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        let mut reply = Vec::new();
        client.read_to_end(&mut reply).unwrap();
        assert_eq!(
            reply,
            [5, 0],
            "dropping authorize={authorize} must not send success"
        );
    }
}

#[test]
fn handoff_preserves_prefix_and_does_not_read_the_transport_until_drained() {
    let input = [b"\x05\x00".as_slice(), REPLY, b"early"].concat();
    let (tunnel, _) = client::connect(
        Script::new(input, usize::MAX),
        target(),
        ClientAuth::NoAuthentication,
    )
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
    transport.read_error = Some(std::io::ErrorKind::ConnectionReset);
    let mut tunnel = socks::Stream::from_parts(transport, buffer);
    let mut bytes = [0; 32];
    assert_eq!(tunnel.read(&mut []).unwrap(), 0);
    assert_eq!(tunnel.read(&mut bytes[..1]).unwrap(), 1);
    assert_eq!(bytes[0], b'e');
    assert_eq!(tunnel.read(&mut bytes).unwrap(), 4);
    assert_eq!(&bytes[..4], b"arly");
    assert_eq!(tunnel.get_ref().read_calls, 1);
    tunnel.write_all(b"outgoing").unwrap();
    tunnel.flush().unwrap();
    tunnel.get_mut().flush_error = Some(std::io::ErrorKind::BrokenPipe);
    assert_eq!(
        tunnel.flush().unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    assert!(tunnel.get_ref().written.ends_with(b"outgoing"));
    let error = tunnel.read(&mut bytes).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
    assert!(error.get_ref().is_some(), "retain the original I/O source");
    assert!(
        tunnel.try_into_inner().is_ok(),
        "drained transport can be extracted"
    );
}

#[test]
fn handoff_rejects_partial_bytes_without_consuming_them() {
    use bnb::Source;
    let mut buffer = bnb::BitBuf::new();
    buffer.push(&[0xab, 0xcd]).unwrap();
    buffer.read_bits(4).unwrap();
    let mut tunnel = socks::Stream::from_parts(Script::new([], 1), buffer);
    assert_eq!(tunnel.read(&mut []).unwrap(), 0);
    let error = tunnel.read(&mut [0]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    let (transport, mut buffer) = tunnel.into_parts();
    assert_eq!(transport.read_calls, 0);
    assert_eq!(buffer.read_bits(12).unwrap(), 0xbcd);
}

#[test]
fn incremental_errors_distinguish_empty_eof_truncation_and_transport_failure() {
    let mut script = Script::new([b"\x05\x00".as_slice(), REPLY].concat(), 1);
    script.read_error = Some(std::io::ErrorKind::Interrupted);
    assert!(client::connect(script, target(), ClientAuth::NoAuthentication).is_ok());
    for (input, expected_codec) in [(vec![], false), (vec![5], true)] {
        let error = client::connect(
            Script::new(input, usize::MAX),
            target(),
            ClientAuth::NoAuthentication,
        )
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
        .err()
        .unwrap();
    assert!(matches!(error, Error::Io(error) if error.get_ref().is_some()));
}

#[test]
fn unknown_address_headers_keep_typed_errors_and_rfc_failure_replies() {
    for code in (0..=u8::MAX).filter(|code| ![1, 3, 4].contains(code)) {
        for chunk in [1, usize::MAX] {
            let mut script = Script::new([5, 1, 0, 5, 1, 0, code], chunk);
            assert!(
                matches!(server::exchange(&mut script, &ServerAuth::no_authentication()),
                Err(Error::UnsupportedAddressType(actual)) if actual == code),
                "ATYP {code}, chunk {chunk}"
            );
            assert_eq!(script.written, [5, 0, 5, 8, 0, 1, 0, 0, 0, 0, 0, 0]);
            assert!(
                matches!(client::connect(Script::new([5, 0, 5, 0, 0, code], chunk), target(), ClientAuth::NoAuthentication),
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
        .err()
        .unwrap();
        assert!(
            matches!(error, Error::Codec(error) if matches!(error.kind, bnb::ErrorKind::UnexpectedEof { .. })),
            "ATYP {code} has a truncated payload"
        );
    }
}

#[test]
fn ipv6_and_max_domain_frames_preserve_tunnel_data() {
    for (destination, request) in support::variable_address_vectors() {
        let mut reply = request.clone();
        reply[1] = 0;
        let mut script = Script::new([b"\x05\x00".as_slice(), &reply, b"early"].concat(), 1);
        let (mut tunnel, bound) = client::connect(
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
        let request = server::exchange(&mut script, &ServerAuth::no_authentication()).unwrap();
        assert_eq!(request.destination(), &destination);
        request
            .send_success(destination)
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
        let (mut tunnel, bound) =
            client::connect(&mut script, target(), ClientAuth::NoAuthentication).unwrap();
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
fn server_defers_success_until_caller_replies_and_preserves_early_payload() {
    for chunk in [1, usize::MAX] {
        let mut script = Script::new(
            [b"\x05\x01\x00".as_slice(), REQUEST, b"early"].concat(),
            chunk,
        );
        let request = server::exchange(&mut script, &ServerAuth::no_authentication()).unwrap();
        assert_eq!(request.destination(), &target());
        let mut tunnel = request.send_success(target()).unwrap();
        let mut payload = Vec::new();
        tunnel.read_to_end(&mut payload).unwrap();
        assert_eq!(payload, b"early");
        assert_eq!(script.written, [b"\x05\x00".as_slice(), REPLY].concat());
    }
    let mut script = Script::new([b"\x05\x01\x00".as_slice(), REQUEST].concat(), 1);
    server::exchange(&mut script, &ServerAuth::no_authentication())
        .unwrap()
        .send_failure(ReplyCode::ConnectionNotAllowed)
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
    client::connect(&mut script, target(), auth).unwrap();
    assert_eq!(
        script.written,
        [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat()
    );
    let mut downgraded = Script::new([5, 0], 1);
    assert!(matches!(
        client::connect(&mut downgraded, target(), auth),
        Err(Error::UnexpectedMethod(_))
    ));
    assert_eq!(downgraded.written, [5, 1, 2]);
    let policy = ServerAuth::username_password(|user, pass| user == b"u" && pass == b"p");
    let mut script = Script::new(
        [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), REQUEST].concat(),
        1,
    );
    server::exchange(&mut script, &policy)
        .unwrap()
        .send_success(target())
        .unwrap();
    assert_eq!(
        script.written,
        [b"\x05\x02\x01\x00".as_slice(), REPLY].concat()
    );
    let mut no_auth = Script::new([5, 1, 0], 1);
    assert!(matches!(
        server::exchange(&mut no_auth, &policy),
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
            server::exchange(&mut script, &policy),
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
            client::connect(&mut script, target(), auth),
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
            client::connect(
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
    client::connect(
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
fn maximum_credentials_complete_server_authentication() {
    for chunk in [1, 2, 257, 513, usize::MAX] {
        let mut script = Script::new(support::maximum_credentials_transcript(), chunk);
        let policy = ServerAuth::username_password(|user, password| {
            user == [7; 255] && password == [8; 255]
        });

        server::exchange(&mut script, &policy).unwrap();

        assert!(
            script.max_read_len <= 513,
            "bounded handshake reads, chunk {chunk}"
        );
        assert_eq!(script.written, [5, 2, 1, 0], "chunk {chunk}");
    }
}

#[test]
fn shortfall_wait_finishes_truncated_variable_fields_at_eof() {
    let input = support::maximum_credentials_transcript();
    let policy =
        ServerAuth::username_password(|_, _| panic!("truncated credentials reached policy"));
    for chunk in [1, 16, usize::MAX] {
        for end in [5, 20, 260, 280, 515] {
            let mut script = Script::new(input[..end].to_vec(), chunk);
            let result = server::exchange(&mut script, &policy);
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
                    client::connect(script, endpoint.clone(), ClientAuth::NoAuthentication);
                assert!(
                    matches!(result, Err(Error::Codec(error)) if matches!(error.kind, bnb::ErrorKind::UnexpectedEof { .. })),
                    "reply {endpoint:?}, prefix {end}, chunk {chunk}"
                );
            }
        }
    }
}

#[test]
fn shortfall_wait_retries_interruptions_and_preserves_transport_errors() {
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
            let result = server::exchange(&mut script, &policy);
            if kind == std::io::ErrorKind::Interrupted {
                let mut tunnel = result.unwrap().send_success(target()).unwrap();
                let mut tail = [0; 4];
                tunnel.read_exact(&mut tail).unwrap();
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

#[test]
fn all_truncated_handshake_prefixes_fail_without_panicking() {
    let client = [b"\x05\x02\x01\x00".as_slice(), REPLY].concat();
    for end in 0..client.len() {
        assert!(
            client::connect(
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
            server::exchange(
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
        assert!(server::exchange(&mut script, &ServerAuth::no_authentication()).is_err());
        assert_eq!(script.written[3], code, "offset {offset}, byte {byte}");
    }
    for (offset, byte) in [(0, 4), (2, 1), (1, 255), (3, 99)] {
        let mut reply = REPLY.to_vec();
        reply[offset] = byte;
        assert!(
            client::connect(
                Script::new([b"\x05\x00".as_slice(), &reply].concat(), 1),
                target(),
                ClientAuth::NoAuthentication
            )
            .is_err()
        );
    }
    for greeting in [vec![5, 0], vec![5, 1, 255], vec![5, 1, 99]] {
        let mut script = Script::new(greeting, 1);
        assert!(server::exchange(&mut script, &ServerAuth::no_authentication()).is_err());
        assert_eq!(script.written, [5, 255]);
    }
}

#[test]
fn server_checks_header_then_capability_then_destination_without_restricting_decode() {
    let reject = |wire: &[u8], reply_code: u8| {
        // The raw codec must retain these requests even when the guided server rejects them.
        let decoded = socks::v5::Request::decode_exact(wire).unwrap();
        assert_eq!(decoded.to_bytes().unwrap(), wire);
        let mut script = Script::new([b"\x05\x01\x00".as_slice(), wire].concat(), 1);
        let error = server::exchange(&mut script, &ServerAuth::no_authentication())
            .err()
            .expect("the guided server must reject this request");
        assert_eq!(
            script.written,
            [5, 0, 5, reply_code, 0, 1, 0, 0, 0, 0, 0, 0],
            "request {wire:?}"
        );
        error
    };

    // Each request has an empty domain and zero port; earlier faults take precedence.
    assert!(matches!(
        reject(&[4, 2, 1, 3, 0, 0, 0], 1),
        Error::Version {
            expected: 5,
            actual: 4
        }
    ));
    assert!(matches!(
        reject(&[5, 2, 1, 3, 0, 0, 0], 1),
        Error::Reserved(1)
    ));
    assert!(matches!(
        reject(&[5, 2, 0, 3, 0, 0, 0], 7),
        Error::UnsupportedCommand(socks::v5::Command::Bind)
    ));
    assert!(matches!(
        reject(&[5, 1, 0, 3, 0, 0, 0], 4),
        Error::InvalidEndpoint
    ));
}

#[test]
fn client_checks_authentication_version_before_status() {
    for version in [0, 5, 255] {
        for status in [0, 1, 255] {
            let mut script = Script::new([5, 2, version, status], 1);
            let result = client::connect(
                &mut script,
                target(),
                ClientAuth::UsernamePassword {
                    username: b"u",
                    password: b"p",
                },
            );
            assert!(
                matches!(result, Err(Error::Version { expected: 1, actual }) if actual == version),
                "version {version}, status {status}"
            );
            assert_eq!(script.written, [5, 1, 2, 1, 1, b'u', 1, b'p']);
        }
    }
}

#[test]
fn client_reports_failure_reply_before_invalid_bound() {
    // An unknown failing REP with an empty domain is still a representable wire reply.
    let wire = [5, 255, 0, 3, 0, 0, 80];
    let reply = socks::v5::Reply::decode_exact(&wire).unwrap();
    assert_eq!(reply.to_bytes().unwrap(), wire);
    let mut script = Script::new([b"\x05\x00".as_slice(), &wire].concat(), 1);
    assert!(matches!(
        client::connect(&mut script, target(), ClientAuth::NoAuthentication),
        Err(Error::Reply(ReplyCode::Other(255)))
    ));
    assert_eq!(
        script.written,
        [b"\x05\x01\x00".as_slice(), REQUEST].concat()
    );
}

#[test]
fn pipelined_payload_and_fin_survive_full_proxy_handoff() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let target_addr = target.local_addr().unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(proxy.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let payload: Vec<u8> = (0..=255).cycle().take(8192).collect();
    let input = [
        b"\x05\x01\x00\x05\x01\x00\x01\x7f\x00\x00\x01".as_slice(),
        &target_addr.port().to_be_bytes(),
        &payload,
    ]
    .concat();
    // Queue handshake, payload larger than the receive cap, and FIN before accept.
    client.write_all(&input).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    std::thread::scope(|scope| {
        let destination = scope.spawn(|| {
            let (mut stream, _) = target.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut received = Vec::new();
            stream.read_to_end(&mut received).unwrap();
            assert_eq!(
                received, payload,
                "deliver prefetched and unread bytes once, in order"
            );
            stream.write_all(b"response after FIN").unwrap();
        });
        let relay = scope.spawn(|| {
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
                proxy.accept().unwrap().0,
                &socks::Server::new(config).unwrap(),
            )
            .unwrap();
        });
        let mut replies = [0; 12];
        client.read_exact(&mut replies).unwrap();
        assert_eq!(&replies[..6], &[5, 0, 5, 0, 0, 1]);
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        assert_eq!(response, b"response after FIN");
        destination.join().unwrap();
        relay.join().unwrap();
    });
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
        let server = socks::Server::builder()
            .protocols([socks::Version::V5])
            .policy(socks::server::policy::Policy::new(
                ServerAuth::no_authentication(),
                move |context| {
                    let (_, _, resolved) =
                        (context.peer, &context.request.destination, context.target);

                    resolved == target_addr
                },
            ))
            .build()
            .unwrap();
        let shutdown_ref = &shutdown;
        let worker =
            scope.spawn(move || proxy::serve(proxy, &server, shutdown_ref, |_| {}).unwrap());
        let (mut tunnel, bound) = client::connect_tcp(
            proxy_addr,
            target_addr.into(),
            ClientAuth::NoAuthentication,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_ne!(bound.port(), 0);
        tunnel
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        tunnel.write_all(b"half-close").unwrap();
        tunnel.get_ref().shutdown(Shutdown::Write).unwrap();
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
        proxy::serve_connection(
            socket,
            &socks::Server::new(ServerConfig::new(
                [socks::Version::V5],
                socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
                    let (_, _, _) = (context.peer, &context.request.destination, context.target);
                    false
                }),
            ))
            .unwrap(),
        )
    });
    assert!(matches!(
        client::connect_tcp(
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
    let mut config = ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            false
        }),
    );
    config.limits.handshake = Duration::from_millis(20);
    let result = proxy::serve_connection(socket, &socks::Server::new(config).unwrap());
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
            server::exchange(&mut script, &policy),
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
        let mut config = ServerConfig::new(
            [socks::Version::V5],
            socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
                let (_, _, _) = (context.peer, &context.request.destination, context.target);
                false
            }),
        );
        config.limits.connections = 1;
        let stop = &shutdown;
        let worker = scope.spawn(move || {
            proxy::serve(listener, &socks::Server::new(config).unwrap(), stop, |_| {})
        });
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
        let mut config = ServerConfig::new(
            [socks::Version::V5],
            socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
                let (_, _, resolved) = (context.peer, &context.request.destination, context.target);

                resolved == destination
            }),
        );
        config.limits.relay = Duration::from_millis(20);
        proxy::serve_connection(
            proxy.accept().unwrap().0,
            &socks::Server::new(config).unwrap(),
        )
    });
    let (mut client, _) = client::connect_tcp(
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
        .get_ref()
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
        let config = ServerConfig::new(
            [socks::Version::V5],
            socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
                let (_, _, resolved) = (context.peer, &context.request.destination, context.target);

                resolved == destination
            }),
        );
        let result = proxy::serve(
            proxy,
            &socks::Server::new(config).unwrap(),
            &shutdown,
            |_| {},
        );
        finished.send(result).unwrap();
    });
    let (mut client, _) = client::connect_tcp(
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
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    target
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut byte = [0];
    assert_eq!(client.read(&mut byte).unwrap(), 0);
    assert_eq!(target.read(&mut byte).unwrap(), 0);
}
