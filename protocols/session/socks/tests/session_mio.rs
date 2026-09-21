//! Fast-tier RFC transcripts and full-tier ephemeral-loopback Mio composition.
use socks::{
    error::Error,
    server::policy::ServerAuth,
    v5::Endpoint,
    v5::auth::ClientAuth,
    v5::{
        client::mio::Client,
        server::mio::{Exchange, Stage},
    },
};
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

// RFC 1928 §§3–6 and RFC 1929 §2, assembled independently of the wire encoder.
const CONNECT: &[u8] = &[5, 1, 0, 1, 127, 0, 0, 1, 0, 80];
const SUCCESS: &[u8] = &[5, 0, 0, 1, 127, 0, 0, 1, 0, 80];

#[test]
fn unsupported_greeting_versions_never_authenticate_or_send_a_guessed_reply() {
    let auth =
        ServerAuth::username_password(|_, _| panic!("foreign protocol reached credential policy"));
    for version in (0..=u8::MAX).filter(|version| *version != 5) {
        let (io, output) = Fragmented::new(vec![version, 1, 2, 1, 1, b'u', 1, b'p'], 1);
        let mut server = Exchange::new(io, auth.clone());
        let mut failure = None;
        for _ in 0..64 {
            if let Err(error) = server.advance() {
                failure = Some(error);
                break;
            }
        }
        assert!(
            matches!(failure, Some(Error::VersionNotAccepted(actual)) if actual == version),
            "version {version}"
        );
        assert!(
            output.lock().unwrap().is_empty(),
            "version {version} must not receive a SOCKS5 reply"
        );
        assert!(
            matches!(server.advance(), Err(Error::InvalidState)),
            "failure must poison the exchange"
        );
    }
}

#[cfg(all(feature = "blocking", feature = "tokio"))]
#[test]
fn complete_backends_share_one_policy_and_verified_neutral_context() {
    use socks::{
        Destination, Version,
        server::policy::{Authentication, Operation, Policy},
    };
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let destination = target.local_addr().unwrap();
    let verifications = Arc::new(AtomicUsize::new(0));
    let authorizations = Arc::new(AtomicUsize::new(0));
    let verified = verifications.clone();
    let authorized = authorizations.clone();
    let policy = Policy::new(
        ServerAuth::username_password(move |user, password| {
            verified.fetch_add(1, Ordering::Relaxed);
            user == b"u" && password == b"p"
        }),
        move |context| {
            assert!(context.peer.ip().is_loopback());
            assert_eq!(context.request.version, Version::V5);
            assert_eq!(
                context.request.authentication,
                Authentication::UsernamePassword
            );
            assert_eq!(context.request.operation, Operation::Connect);
            assert_eq!(context.request.destination, Destination::from(destination));
            assert_eq!(context.target, destination);
            authorized.fetch_add(1, Ordering::Relaxed);
            false
        },
    );
    let server = Server::builder()
        .protocols([Version::V5])
        .policy(policy)
        .limits(socks::server::Limits {
            relay: Duration::from_secs(5),
            ..Default::default()
        })
        .build()
        .unwrap();
    let client = socks::Client::builder()
        .protocol(socks::v5::client::Config::username_password(b"u", b"p").unwrap())
        .build()
        .unwrap();
    for backend in 0..3 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = server.clone();
        let worker = thread::spawn(move || {
            if backend == 2 {
                listener.set_nonblocking(true).unwrap();
                let mut proxy =
                    Proxy::new(mio::net::TcpListener::from_std(listener), server).unwrap();
                let stop = proxy.shutdown_handle();
                proxy
                    .run(|error| {
                        assert!(matches!(error, Error::PermissionDenied));
                        stop.shutdown().unwrap();
                    })
                    .unwrap();
            } else {
                let (stream, _) = listener.accept().unwrap();
                let result = if backend == 0 {
                    socks::proxy::blocking::serve_connection(stream, &server)
                } else {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    runtime.block_on(async {
                        stream.set_nonblocking(true).unwrap();
                        socks::proxy::tokio::serve_connection(
                            tokio::net::TcpStream::from_std(stream).unwrap(),
                            &server,
                        )
                        .await
                    })
                };
                assert!(matches!(result, Err(Error::PermissionDenied)));
            }
        });
        assert!(matches!(
            client.connect_tcp(address, destination.into()),
            Err(Error::Reply(socks::v5::ReplyCode::ConnectionNotAllowed))
        ));
        worker.join().unwrap();
        assert_eq!(verifications.load(Ordering::Relaxed), backend + 1);
        assert_eq!(authorizations.load(Ordering::Relaxed), backend + 1);
        assert!(
            matches!(target.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock),
            "policy denial must prevent dialing on backend {backend}"
        );
    }
}

struct Fragmented {
    input: VecDeque<u8>,
    output: Arc<Mutex<Vec<u8>>>,
    chunk: usize,
    operations: usize,
}

impl Fragmented {
    fn new(input: Vec<u8>, chunk: usize) -> (Self, Arc<Mutex<Vec<u8>>>) {
        let output = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                input: input.into(),
                output: output.clone(),
                chunk,
                operations: 0,
            },
            output,
        )
    }
    fn step(&mut self) -> io::Result<()> {
        self.operations += 1;
        match self.operations % 3 {
            0 => Ok(()),
            1 => Err(io::ErrorKind::WouldBlock.into()),
            _ => Err(io::ErrorKind::Interrupted.into()),
        }
    }
}
impl Read for Fragmented {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.step()?;
        let count = bytes.len().min(self.chunk).min(self.input.len());
        for byte in &mut bytes[..count] {
            *byte = self.input.pop_front().unwrap();
        }
        Ok(count)
    }
}
impl Write for Fragmented {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.step()?;
        let count = bytes.len().min(self.chunk);
        self.output
            .lock()
            .unwrap()
            .extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.step()
    }
}

fn client_done<S: Read + Write>(client: &mut Client<S>) {
    for _ in 0..10000 {
        if client.advance().unwrap() {
            return;
        }
    }
    panic!("bounded transcript did not complete");
}
fn exchange_stage<S: Read + Write>(server: &mut Exchange<S>) -> Result<Stage, Error> {
    for _ in 0..10000 {
        match server.advance()? {
            Stage::Exchanging => {}
            stage => return Ok(stage),
        }
    }
    panic!("bounded transcript did not complete");
}

#[test]
fn connect_rejects_zero_ports_before_client_io_or_server_handoff() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    for wire in [
        vec![5, 1, 0, 1, 127, 0, 0, 1, 0, 0],
        vec![5, 1, 0, 3, 1, 0xff, 0, 0],
        vec![
            5, 1, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0,
        ],
    ] {
        let destination = socks::v5::Request::decode_exact(&wire).unwrap().destination;
        let (mut io, output) = Fragmented::new(vec![], 1);
        assert!(matches!(
            Client::new(&mut io, destination.clone(), ClientAuth::NoAuthentication),
            Err(Error::ZeroDestinationPort)
        ));
        assert_eq!(io.operations, 0);
        assert!(output.lock().unwrap().is_empty());
        assert!(matches!(
            socks::v5::client::mio::connect_tcp(
                proxy.local_addr().unwrap(),
                destination,
                ClientAuth::NoAuthentication,
                Duration::from_secs(1)
            ),
            Err(Error::ZeroDestinationPort)
        ));
        assert!(matches!(proxy.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));

        let (io, output) = Fragmented::new([b"\x05\x01\x00".as_slice(), &wire].concat(), 1);
        let mut server = Exchange::new(io, ServerAuth::no_authentication());
        assert!(matches!(
            exchange_stage(&mut server),
            Err(Error::ZeroDestinationPort)
        ));
        assert!(server.destination().is_none());
        assert_eq!(
            *output.lock().unwrap(),
            [5, 0, 5, 1, 0, 1, 0, 0, 0, 0, 0, 0]
        );
    }
}

#[test]
fn client_retries_partial_reads_writes_and_flush_without_repeating_bytes() {
    for chunk in 1..=32 {
        let input = [b"\x05\x02\x01\x00".as_slice(), SUCCESS, b"payload"].concat();
        let (io, output) = Fragmented::new(input, chunk);
        let destination = Endpoint::domain(b"example.org", 443).unwrap();
        let mut client = Client::new(
            io,
            destination,
            ClientAuth::UsernamePassword {
                username: b"u",
                password: b"p",
            },
        )
        .unwrap();
        assert!(client.take_stream().is_none());
        client_done(&mut client);
        let (mut stream, bound) = client.take_stream().unwrap();
        assert_eq!(
            bound,
            "127.0.0.1:80"
                .parse::<std::net::SocketAddr>()
                .unwrap()
                .into()
        );
        let expected = [
            b"\x05\x01\x02\x01\x01u\x01p\x05\x01\x00\x03\x0b".as_slice(),
            b"example.org",
            b"\x01\xbb",
        ]
        .concat();
        assert_eq!(*output.lock().unwrap(), expected, "chunk={chunk}");
        let mut payload = Vec::new();
        loop {
            let mut bytes = [0; 32];
            match stream.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => payload.extend_from_slice(&bytes[..count]),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => panic!("{error}"),
            }
        }
        assert_eq!(payload, b"payload", "chunk={chunk}");
        assert!(client.take_stream().is_none());
        assert!(matches!(client.advance(), Err(Error::InvalidState)));
    }
}

#[test]
fn server_authenticates_once_and_defers_success_until_explicit_decision() {
    for chunk in 1..=32 {
        let input = [
            b"\x05\x01\x02\x01\x01u\x01p".as_slice(),
            CONNECT,
            b"payload",
        ]
        .concat();
        let (io, output) = Fragmented::new(input, chunk);
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let auth = ServerAuth::username_password(move |user, pass| {
            counter.fetch_add(1, Ordering::Relaxed);
            user == b"u" && pass == b"p"
        });
        let mut server = Exchange::new(io, auth);
        assert!(matches!(
            server.send_success(
                "127.0.0.1:80"
                    .parse::<std::net::SocketAddr>()
                    .unwrap()
                    .into()
            ),
            Err(Error::InvalidState)
        ));
        assert_eq!(exchange_stage(&mut server).unwrap(), Stage::Requested);
        assert_eq!(*output.lock().unwrap(), b"\x05\x02\x01\x00");
        assert_eq!(server.interest(), None);
        for _ in 0..3 {
            assert_eq!(server.advance().unwrap(), Stage::Requested);
        }
        server
            .send_success(
                "127.0.0.1:80"
                    .parse::<std::net::SocketAddr>()
                    .unwrap()
                    .into(),
            )
            .unwrap();
        assert_eq!(exchange_stage(&mut server).unwrap(), Stage::Established);
        assert_eq!(
            *output.lock().unwrap(),
            [b"\x05\x02\x01\x00".as_slice(), SUCCESS].concat()
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(server.take_stream().is_some());
    }
}

use socks::{Server, proxy::mio::Proxy, server::ServerConfig};
use std::{
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    thread,
    time::Duration,
};

struct Running {
    address: SocketAddr,
    stop: socks::proxy::mio::Shutdown,
    thread: Option<thread::JoinHandle<()>>,
    errors: Arc<Mutex<Vec<Error>>>,
}
impl Running {
    fn new(config: ServerConfig) -> Self {
        let listener = mio::net::TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let mut proxy = Proxy::new(listener, Server::new(config).unwrap()).unwrap();
        let address = proxy.local_addr();
        let stop = proxy.shutdown_handle();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let reports = errors.clone();
        let thread = thread::spawn(move || {
            proxy
                .run(|error| reports.lock().unwrap().push(error))
                .unwrap();
        });
        Self {
            address,
            stop,
            thread: Some(thread),
            errors,
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop.shutdown().unwrap();
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
fn tcp(address: SocketAddr) -> TcpStream {
    let stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
}
fn request(address: SocketAddr) -> Vec<u8> {
    [
        b"\x05\x01\x00\x01\x7f\x00\x00\x01".as_slice(),
        &address.port().to_be_bytes(),
    ]
    .concat()
}

#[test]
fn proxy_preserves_pipelined_payload_and_client_half_close() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    let payload = vec![0x5a; 256 * 1024];
    let expected = payload.clone();
    let worker = thread::spawn(move || {
        let (mut stream, peer) = target.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut received = Vec::new();
        stream.read_to_end(&mut received).unwrap();
        assert_eq!(received, expected);
        stream.write_all(b"response after FIN").unwrap();
        peer
    });
    let proxy = Running::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
            let (_, _, resolved) = (context.peer, &context.request.destination, context.target);
            resolved == address
        }),
    ));
    let mut client = tcp(proxy.address);
    client
        .write_all(&[b"\x05\x01\x00".as_slice(), &request(address), &payload].concat())
        .unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let mut received = Vec::new();
    client.read_to_end(&mut received).unwrap();
    let bound = worker.join().unwrap();
    assert_eq!(&received[..10], b"\x05\x00\x05\x00\x00\x01\x7f\x00\x00\x01");
    assert_eq!(&received[10..12], &bound.port().to_be_bytes());
    assert_eq!(&received[12..], b"response after FIN");
    assert!(proxy.errors.lock().unwrap().is_empty());
}

#[test]
fn proxy_preserves_target_half_close_while_client_keeps_writing() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut stream, _) = target.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.write_all(b"early response").unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut received = Vec::new();
        stream.read_to_end(&mut received).unwrap();
        received
    });
    let proxy = Running::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
            let (_, _, resolved) = (context.peer, &context.request.destination, context.target);
            resolved == address
        }),
    ));
    let mut client = tcp(proxy.address);
    client
        .write_all(&[b"\x05\x01\x00".as_slice(), &request(address)].concat())
        .unwrap();
    let mut received = Vec::new();
    client.read_to_end(&mut received).unwrap();
    assert_eq!(&received[12..], b"early response");
    client.write_all(b"after target FIN").unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    assert_eq!(worker.join().unwrap(), b"after target FIN");
}

#[test]
fn proxy_denied_target_is_never_dialed() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let address = target.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let proxy = Running::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
            let (_, original, resolved) =
                (context.peer, &context.request.destination, context.target);

            assert_eq!(original, &address.into());
            assert_eq!(resolved, address);
            counter.fetch_add(1, Ordering::Relaxed);
            false
        }),
    ));
    let mut client = tcp(proxy.address);
    client
        .write_all(&[b"\x05\x01\x00".as_slice(), &request(address)].concat())
        .unwrap();
    let mut received = Vec::new();
    client.read_to_end(&mut received).unwrap();
    assert_eq!(
        received,
        b"\x05\x00\x05\x02\x00\x01\x00\x00\x00\x00\x00\x00"
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        target.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn mio_client_connects_to_proxy_and_shutdown_closes_both_sides() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    let proxy = Running::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
            let (_, _, resolved) = (context.peer, &context.request.destination, context.target);
            resolved == address
        }),
    ));
    let configured = socks::Client::builder()
        .protocol(socks::v5::client::Config::no_authentication())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let (mut poll, mut client, _) = configured
        .connect_tcp_mio(proxy.address, address.into())
        .unwrap();
    let (mut target, _) = target.accept().unwrap();
    target
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    proxy.stop.shutdown().unwrap();
    let mut byte = [0];
    assert_eq!(target.read(&mut byte).unwrap(), 0);
    let mut events = mio::Events::with_capacity(8);
    poll.registry()
        .register(client.get_mut(), mio::Token(0), mio::Interest::READABLE)
        .unwrap();
    loop {
        match client.read(&mut byte) {
            Ok(0) => break,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                poll.poll(&mut events, Some(Duration::from_secs(5)))
                    .unwrap();
                assert!(!events.is_empty(), "shutdown must close the client too");
            }
            result => panic!("unexpected shutdown result: {result:?}"),
        }
    }
}

#[test]
fn proxy_resolves_domain_off_loop_and_authorizes_numeric_addresses() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    let proxy = Running::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(
            ServerAuth::username_password(|u, p| u == b"u" && p == b"p"),
            move |context| {
                let (_, original, resolved) =
                    (context.peer, &context.request.destination, context.target);

                assert_eq!(
                    original,
                    &socks::Destination::domain(b"localhost", address.port())
                );
                resolved == address
            },
        ),
    ));
    let (_poll, _client, _) = socks::v5::client::mio::connect_tcp(
        proxy.address,
        Endpoint::domain(b"localhost", address.port()).unwrap(),
        ClientAuth::UsernamePassword {
            username: b"u",
            password: b"p",
        },
        Duration::from_secs(5),
    )
    .unwrap();
    let (_target, _) = target.accept().unwrap();
}

#[test]
fn silent_handshake_expires_without_readiness_and_capacity_recovers() {
    let mut config = ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), |context| {
            let (_, _, _) = (context.peer, &context.request.destination, context.target);
            false
        }),
    );
    config.limits.connections = 1;
    config.limits.handshake = Duration::from_millis(50);
    let proxy = Running::new(config);
    let mut silent = tcp(proxy.address);
    let mut next = tcp(proxy.address);
    next.write_all(b"\x05\x01\x00").unwrap();
    assert_eq!(silent.read(&mut [0]).unwrap(), 0);
    let mut selection = [0; 2];
    next.read_exact(&mut selection).unwrap();
    assert_eq!(
        selection,
        [5, 0],
        "freeing a slot must resume the listener without a new accept edge"
    );
}

#[test]
fn invalid_credentials_never_reach_callback_and_failure_is_fully_written() {
    for credentials in [
        b"\x01\x00\x01p".as_slice(),
        b"\x02\x01u\x01p",
        b"\x01\x01u\x00",
    ] {
        let input = [b"\x05\x01\x02".as_slice(), credentials, CONNECT].concat();
        let (io, output) = Fragmented::new(input, 1);
        let mut server = Exchange::new(
            io,
            ServerAuth::username_password(|_, _| {
                panic!("invalid credentials must not reach policy")
            }),
        );
        assert!(matches!(
            exchange_stage(&mut server),
            Err(Error::AuthenticationRejected)
        ));
        assert_eq!(*output.lock().unwrap(), b"\x05\x02\x01\x01");
        assert!(matches!(server.advance(), Err(Error::InvalidState)));
        assert_eq!(server.interest(), None);
    }
}

#[test]
fn no_authentication_is_not_a_username_password_fallback() {
    let (io, output) = Fragmented::new(vec![5, 0], 1);
    let mut client = Client::new(
        io,
        "127.0.0.1:80".parse::<SocketAddr>().unwrap().into(),
        ClientAuth::UsernamePassword {
            username: b"u",
            password: b"p",
        },
    )
    .unwrap();
    let error = (0..10000)
        .find_map(|_| client.advance().err())
        .expect("unoffered selection must fail without waiting for more input");
    assert!(matches!(error, Error::UnexpectedMethod(_)));
    assert_eq!(*output.lock().unwrap(), [5, 1, 2]);
    assert_eq!(client.interest(), None);
}

#[test]
fn all_unknown_address_types_get_reply_eight_without_parsing_a_payload() {
    for atyp in 0..=255 {
        if [1, 3, 4].contains(&atyp) {
            continue;
        }
        let (io, output) = Fragmented::new(vec![5, 1, 0, 5, 1, 0, atyp], 1);
        let mut server = Exchange::new(io, ServerAuth::no_authentication());
        assert!(
            matches!(exchange_stage(&mut server), Err(Error::UnsupportedAddressType(code)) if code == atyp)
        );
        assert_eq!(
            *output.lock().unwrap(),
            b"\x05\x00\x05\x08\x00\x01\x00\x00\x00\x00\x00\x00",
            "atyp={atyp}"
        );
    }
}

#[test]
fn every_truncated_handshake_is_terminal_not_a_retryable_eof() {
    let input = [b"\x05\x01\x02\x01\x01u\x01p".as_slice(), CONNECT].concat();
    for length in 0..input.len() {
        let (io, _) = Fragmented::new(input[..length].to_vec(), 1);
        let mut server = Exchange::new(io, ServerAuth::username_password(|_, _| true));
        assert!(exchange_stage(&mut server).is_err(), "length={length}");
        assert!(
            matches!(server.advance(), Err(Error::InvalidState)),
            "length={length}"
        );
    }
}

#[test]
fn maximum_credentials_survive_fairness_yields_and_fragmentation() {
    let input = [
        b"\x05\x01\x02\x01\xff".as_slice(),
        &[b'u'; 255],
        b"\xff",
        &[b'p'; 255],
        CONNECT,
    ]
    .concat();
    let (io, _) = Fragmented::new(input, 1);
    let mut server = Exchange::new(
        io,
        ServerAuth::username_password(|u, p| u == [b'u'; 255] && p == [b'p'; 255]),
    );
    assert_eq!(exchange_stage(&mut server).unwrap(), Stage::Requested);
}

struct Interrupted(Arc<AtomicUsize>);
impl Read for Interrupted {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        assert!(
            self.0.fetch_add(1, Ordering::Relaxed) < 64,
            "one advance must bound transport operations"
        );
        Err(io::ErrorKind::Interrupted.into())
    }
}
impl Write for Interrupted {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        assert!(
            self.0.fetch_add(1, Ordering::Relaxed) < 64,
            "one advance must bound transport operations"
        );
        Err(io::ErrorKind::Interrupted.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        assert!(
            self.0.fetch_add(1, Ordering::Relaxed) < 64,
            "one advance must bound transport operations"
        );
        Err(io::ErrorKind::Interrupted.into())
    }
}

#[test]
fn interrupted_transports_yield_with_explicit_continuation_and_bounded_work() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut server = Exchange::new(Interrupted(calls.clone()), ServerAuth::no_authentication());
    for _ in 0..3 {
        calls.store(0, Ordering::Relaxed);
        assert_eq!(server.advance().unwrap(), Stage::Exchanging);
        assert!(
            server.needs_advance(),
            "budget exhaustion must not wait for another readiness edge"
        );
        assert!((1..=64).contains(&calls.load(Ordering::Relaxed)));
    }
    let mut client = Client::new(
        Interrupted(calls.clone()),
        "127.0.0.1:80".parse::<SocketAddr>().unwrap().into(),
        ClientAuth::NoAuthentication,
    )
    .unwrap();
    calls.store(0, Ordering::Relaxed);
    assert!(!client.advance().unwrap());
    assert!(client.needs_advance());
    assert!((1..=64).contains(&calls.load(Ordering::Relaxed)));
}

#[test]
fn real_would_block_waits_for_readiness_instead_of_busy_polling() {
    let (io, _) = Fragmented::new(Vec::new(), 1);
    let mut server = Exchange::new(io, ServerAuth::no_authentication());
    assert_eq!(server.advance().unwrap(), Stage::Exchanging);
    assert!(!server.needs_advance());
    assert_eq!(server.interest(), Some(mio::Interest::READABLE));
    let (io, _) = Fragmented::new(Vec::new(), 1);
    let mut client = Client::new(
        io,
        "127.0.0.1:80".parse::<SocketAddr>().unwrap().into(),
        ClientAuth::NoAuthentication,
    )
    .unwrap();
    assert!(!client.advance().unwrap());
    assert!(!client.needs_advance());
    assert_eq!(client.interest(), Some(mio::Interest::WRITABLE));
}

#[test]
fn explicit_failure_cannot_be_success_or_follow_a_queued_success() {
    let (io, output) = Fragmented::new([b"\x05\x01\x00".as_slice(), CONNECT].concat(), 1);
    let mut server = Exchange::new(io, ServerAuth::no_authentication());
    assert_eq!(exchange_stage(&mut server).unwrap(), Stage::Requested);
    for code in [
        socks::v5::ReplyCode::Succeeded,
        socks::v5::ReplyCode::Other(0),
    ] {
        assert!(server.send_failure(code).is_err());
    }
    server
        .send_success("127.0.0.1:80".parse::<SocketAddr>().unwrap().into())
        .unwrap();
    assert!(matches!(
        server.send_failure(socks::v5::ReplyCode::GeneralFailure),
        Err(Error::InvalidState)
    ));
    assert_eq!(exchange_stage(&mut server).unwrap(), Stage::Established);
    assert_eq!(
        *output.lock().unwrap(),
        [b"\x05\x00".as_slice(), SUCCESS].concat()
    );
}

#[test]
fn connector_uses_an_absolute_deadline_and_poisoning_is_terminal() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let until = std::time::Instant::now() + Duration::from_millis(50);
    let mut connector =
        socks::io::mio::Connector::new(target.local_addr().unwrap(), until).unwrap();
    let mut poll = mio::Poll::new().unwrap();
    let mut events = mio::Events::with_capacity(8);
    poll.registry()
        .register(
            connector.transport_mut().unwrap(),
            mio::Token(0),
            mio::Interest::WRITABLE,
        )
        .unwrap();
    while std::time::Instant::now() < until {
        poll.poll(
            &mut events,
            Some(until.saturating_duration_since(std::time::Instant::now())),
        )
        .unwrap();
    }
    assert!(
        matches!(connector.advance(), Err(Error::Io(error)) if error.kind() == io::ErrorKind::TimedOut)
    );
    assert!(connector.take_stream().is_none());
    assert!(connector.transport_mut().is_err());
}

#[test]
fn refused_outbound_connect_gets_a_failure_reply_not_success() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    drop(target);
    let proxy = Running::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
            let (_, _, resolved) = (context.peer, &context.request.destination, context.target);
            resolved == address
        }),
    ));
    let result = socks::v5::client::mio::connect_tcp(
        proxy.address,
        address.into(),
        ClientAuth::NoAuthentication,
        Duration::from_secs(5),
    );
    assert!(matches!(
        result,
        Err(Error::Reply(socks::v5::ReplyCode::ConnectionRefused))
    ));
}

#[cfg(feature = "blocking")]
#[test]
fn blocking_client_interoperates_with_mio_proxy() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    let proxy = Running::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
            let (_, _, resolved) = (context.peer, &context.request.destination, context.target);
            resolved == address
        }),
    ));
    let (_stream, _) = socks::v5::client::blocking::connect_tcp(
        proxy.address,
        address.into(),
        ClientAuth::NoAuthentication,
        Duration::from_secs(5),
    )
    .unwrap();
    let (_target, _) = target.accept().unwrap();
}

#[cfg(feature = "tokio")]
#[test]
fn tokio_client_interoperates_with_mio_proxy() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = target.local_addr().unwrap();
    let proxy = Running::new(ServerConfig::new(
        [socks::Version::V5],
        socks::server::policy::Policy::new(ServerAuth::no_authentication(), move |context| {
            let (_, _, resolved) = (context.peer, &context.request.destination, context.target);
            resolved == address
        }),
    ));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let client = socks::Client::builder()
            .protocol(socks::v5::client::Config::no_authentication())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let (_stream, _) = client
            .connect_tcp_async(proxy.address, address.into())
            .await
            .unwrap();
        let (_target, _) = target.accept().unwrap();
    });
}
