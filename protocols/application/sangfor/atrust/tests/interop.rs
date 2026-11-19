//! The network layer against a real TLS server on loopback: `openssl s_server`.
//!
//! Tests that start a server are ignored by default. They need the `openssl` binary and free
//! loopback ports. Run them
//! with `cargo test -p sangfor-atrust --features net --test interop -- --ignored`. It checks
//! what the scripted tests cannot: that the socket, TLS and HTTP layers fit together under
//! `HttpsPortal`, and that a TLS connection is the `Transport` a tunnel needs. No gateway is
//! involved. The release-only bulk test adds a scripted node for the tunnel data phase.

#[path = "support/mod.rs"]
mod support;

#[cfg(not(debug_assertions))]
#[path = "interop/bulk.rs"]
mod bulk;

mod interop {
    use std::{
        env, fs,
        io::{ErrorKind, Read, Write},
        net::{TcpListener, TcpStream},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        thread,
        time::Duration,
    };

    use http1::{Headers, Limits, Request};
    use sangfor_atrust::api::{Call, Context, Gateway, Portal};
    use sangfor_atrust::net::{HttpsPortal, dial};
    use sangfor_atrust::tcp::Transport;

    use tls::ServerVerification;
    use x509::Certificate;

    use super::support::Counting;

    const TIMEOUT: Duration = Duration::from_secs(10);

    /// Kills the server when the test ends, however it ends.
    pub(super) struct Server(pub(super) Child);

    impl Drop for Server {
        fn drop(&mut self) {
            self.0.kill().ok();
            self.0.wait().ok();
        }
    }

    /// Runs `openssl` with `arguments` in `directory`, hiding its output.
    fn openssl(directory: &Path, arguments: &[&str]) -> Command {
        let mut command = Command::new("openssl");
        command
            .args(arguments)
            .current_dir(directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    /// Make the throwaway signing CA and DNS leaf for a loopback peer.
    pub(super) fn certificates(name: &str) -> PathBuf {
        let directory = env::temp_dir().join(format!("atrust-{name}-{}", std::process::id()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).unwrap();
        let made = openssl(
            &directory,
            &[
                "req",
                "-x509",
                "-newkey",
                "ed25519",
                "-nodes",
                "-keyout",
                "ca.key",
                "-out",
                "ca.pem",
                "-days",
                "2",
                "-subj",
                "/CN=Test CA",
                "-addext",
                "basicConstraints=critical,CA:TRUE",
                "-addext",
                "keyUsage=critical,keyCertSign",
            ],
        )
        .status()
        .unwrap();
        assert!(made.success(), "openssl CA creation failed");
        let made = openssl(
            &directory,
            &[
                "req",
                "-CA",
                "ca.pem",
                "-CAkey",
                "ca.key",
                "-addext",
                "subjectAltName=DNS:atrust.test",
                "-addext",
                "keyUsage=critical,digitalSignature",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                "key.pem",
                "-out",
                "cert.pem",
                "-outform",
                "PEM",
                "-days",
                "2",
                "-subj",
                "/CN=atrust.test",
            ],
        )
        .status()
        .unwrap();
        assert!(made.success(), "openssl req failed");
        let der = openssl(
            &directory,
            &[
                "x509", "-in", "cert.pem", "-outform", "DER", "-out", "cert.der",
            ],
        )
        .status()
        .unwrap();
        assert!(der.success(), "openssl x509 failed");

        assert!(
            openssl(
                &directory,
                &["x509", "-in", "ca.pem", "-outform", "DER", "-out", "ca.der"]
            )
            .status()
            .unwrap()
            .success()
        );
        directory
    }

    /// Start the OpenSSL server and wait for it to accept connections.
    fn start_server(name: &str, address: &str, behaviour: &str) -> (Server, PathBuf) {
        let directory = certificates(name);
        let arguments = [
            "s_server", "-accept", address, "-cert", "cert.pem", "-key", "key.pem", "-tls1_2",
            behaviour,
        ];
        let server = Server(openssl(&directory, &arguments).spawn().unwrap());
        for _ in 0..50 {
            if TcpStream::connect(address).is_ok() {
                return (server, directory);
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("openssl s_server did not start listening on {address}");
    }

    #[test]
    fn the_portal_rejects_another_authority_before_connecting() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let gateway = Gateway::new("127.0.0.1", listener.local_addr().unwrap().port());
        let mut portal = HttpsPortal::new(
            &gateway,
            ServerVerification::Unverified,
            Counting::default(),
        )
        .with_timeout(Duration::from_millis(50));
        let matching = Request::get(&gateway.base_host(), "/").unwrap();
        let mut missing = matching.clone();
        missing.headers = Headers::new();
        let mut duplicate = matching.clone();
        duplicate.headers.push("hOsT", gateway.base_host());
        let mut split_field = matching.clone();
        split_field.headers.push("X-Trace", b"first\nsecond");
        let mut split_target = matching.clone();
        split_target.target = b"/two words".to_vec();
        let mut undeclared_body = matching.clone();
        undeclared_body.body = b"extra".to_vec();
        let mut absolute = matching;
        absolute.target = b"https://other.test/".to_vec();
        for request in [
            missing,
            duplicate,
            absolute,
            split_field,
            split_target,
            undeclared_body,
            Request::get(&format!("other.test:{}", gateway.port()), "/").unwrap(),
            Request::get("127.0.0.1:0", "/").unwrap(),
        ] {
            assert_eq!(
                portal.exchange(&request).unwrap_err().kind(),
                ErrorKind::InvalidInput
            );
        }
        assert_eq!(listener.accept().unwrap_err().kind(), ErrorKind::WouldBlock);
    }

    #[test]
    #[ignore = "needs the openssl binary and a free loopback port"]
    fn the_portal_carries_a_request_over_tcp_tls_and_http() {
        // `-www` answers any request with a status page and ends the connection.
        let (_server, directory) = start_server("portal", "127.0.0.1:44491", "-www");
        let gateway = Gateway::new("127.0.0.1", 44491);
        let request = Call::manifest()
            .request(&Context {
                gateway: &gateway,
                csrf_token: "",
                trace_id: "",
                sign_key: None,
                cookie: None,
            })
            .unwrap();

        let ca = fs::read(directory.join("ca.der")).unwrap();
        let anchors = [Certificate::from_der(&ca).unwrap()];
        let verification = |dns_name| ServerVerification::Path {
            trust_anchors: &anchors,
            time: anchors[0].tbs_certificate().validity().not_after,
            dns_name,
        };
        let mut wrong = HttpsPortal::new(&gateway, verification("wrong.test"), Counting::default());
        assert!(wrong.exchange(&request).is_err());
        let mut portal =
            HttpsPortal::new(&gateway, verification("atrust.test"), Counting::default())
                .with_timeout(TIMEOUT);
        let reply = portal.exchange(&request).unwrap();

        assert_eq!(reply.response.head.status, 200);
        assert_ne!(reply.response.body.len(), 0);
        // The chain is what the server presented: its one certificate, in DER.
        assert_eq!(
            reply.peer_chain,
            [fs::read(directory.join("cert.der")).unwrap()]
        );

        // Each exchange is a connection of its own.
        assert_eq!(portal.exchange(&request).unwrap().response.head.status, 200);

        // The limits given are the limits applied: the page does not fit sixteen bytes.
        let mut strict = portal.with_limits(Limits::default().with_body(16));
        assert_eq!(
            strict.exchange(&request).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    #[ignore = "needs the openssl binary and a free loopback port"]
    fn a_tls_connection_stops_writing_and_goes_on_reading() {
        // `-rev` sends each line back reversed.
        let (_server, directory) = start_server("node", "127.0.0.1:44492", "-rev");
        let leaf = fs::read(directory.join("cert.der")).unwrap();
        let leaf = Certificate::from_der(&leaf).unwrap();
        let untrusted = ServerVerification::Path {
            trust_anchors: &[],
            time: leaf.tbs_certificate().validity().not_before,
            dns_name: "atrust.test",
        };
        let error = dial(
            "127.0.0.1:44492",
            TIMEOUT,
            untrusted,
            &mut Counting::default(),
        )
        .err()
        .expect("a node with no trusted issuer must be refused");
        assert!(
            matches!(
                error.get_ref().and_then(|error| error.downcast_ref::<tls::blocking::Error>()),
                Some(tls::blocking::Error::Tls(tls::Error::Certificate(tls::CertificateError::Path(path))))
                    if path.kind == pki::ErrorKind::PathNotFound
            ),
            "{error:?}"
        );
        let mut node = dial(
            "127.0.0.1:44492",
            TIMEOUT,
            tls::ServerVerification::Unverified,
            &mut Counting::default(),
        )
        .unwrap();

        node.write_all(b"hello node\n").unwrap();
        let mut answer = [0; 11];
        node.read_exact(&mut answer).unwrap();
        assert_eq!(&answer, b"edon olleh\n");

        // An unfinished line, then this end's goodbye. The server reverses what it holds and
        // says goodbye in turn: what it sends after ours must still arrive.
        node.write_all(b"abc").unwrap();
        node.close_write().unwrap();
        let mut rest = Vec::new();
        node.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, b"cba\n");
        fs::remove_dir_all(&directory).ok();
    }
}
