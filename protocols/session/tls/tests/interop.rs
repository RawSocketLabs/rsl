//! Live interoperability: the client against a real `openssl s_server` on loopback.
//!
//! Ignored by default because it needs the `openssl` binary and a free loopback port. Run it
//! with `cargo test -p tls --test interop -- --ignored`. It is the test that checks the
//! whole client against an independent implementation, and the source of the recorded
//! connection in `support`: set `TLS_RECORD_DIR` to a directory to write both directions'
//! bytes there as `client.bin` and `server.bin`.

mod support;

mod interop {
    use std::{
        env, fs,
        io::{Read, Write},
        net::TcpStream,
        path::Path,
        process::{Child, Command, Stdio},
        thread,
        time::Duration,
    };

    use tls::{Client, ClientConfig, ServerVerification};

    use super::support::CountingRandom;

    /// `s_server -rev` sends each line back reversed.
    const REQUEST: &[u8] = b"hello tls\n";
    const RESPONSE: &[u8] = b"slt olleh\n";

    /// Kills the server when the test ends, however it ends.
    struct Server(Child);

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

    /// Starts `openssl s_server` with a fresh throwaway certificate and `extra` arguments,
    /// and waits until it accepts connections.
    fn start_server(directory: &Path, address: &str, extra: &[&str]) -> (Server, TcpStream) {
        fs::create_dir_all(directory).unwrap();
        let made = openssl(
            directory,
            &[
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                "key.pem",
                "-out",
                "cert.pem",
                "-days",
                "2",
                "-subj",
                "/CN=tls.test",
            ],
        )
        .status()
        .unwrap();
        assert!(made.success(), "openssl req failed");
        let mut arguments = vec![
            "s_server", "-accept", address, "-cert", "cert.pem", "-key", "key.pem", "-tls1_2",
            "-rev",
        ];
        arguments.extend(extra);
        let server = Server(openssl(directory, &arguments).spawn().unwrap());
        for _ in 0..50 {
            if let Ok(stream) = TcpStream::connect(address) {
                return (server, stream);
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("openssl s_server did not start listening on {address}");
    }

    /// Connects to a fresh server, sends one line, expects it back reversed, and closes.
    /// Returns every byte sent and received.
    fn exchange(name: &str, address: &str, extra: &[&str]) -> (Vec<u8>, Vec<u8>) {
        let directory = env::temp_dir().join(format!("tls-{name}-{}", std::process::id()));
        let (_server, mut stream) = start_server(&directory, address, extra);
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();

        let config = ClientConfig {
            server_name: Some("tls.test"),
            verification: ServerVerification::Unverified,
        };
        let mut client = Client::new(config, &mut CountingRandom::default()).unwrap();
        let (mut sent, mut received, mut response) = (Vec::new(), Vec::new(), Vec::<u8>::new());
        let mut requested = false;
        let mut buffer = [0; 4096];

        while !client.peer_closed() {
            if client.is_established() && !requested {
                assert_eq!(client.write(REQUEST), Ok(REQUEST.len()));
                requested = true;
            }
            let outgoing = client.transmit().to_vec();
            if !outgoing.is_empty() {
                stream.write_all(&outgoing).unwrap();
                client.sent(outgoing.len());
                sent.extend(outgoing);
                continue;
            }
            assert!(
                client.wants_receive(),
                "nothing to send and nothing to wait for"
            );
            let count = stream.read(&mut buffer).unwrap();
            if count == 0 {
                client.receive_eof().unwrap();
                break;
            }
            received.extend(&buffer[..count]);
            let mut offset = 0;
            while offset < count {
                offset += client.receive(&buffer[offset..count]).unwrap();
                response.extend(client.plaintext());
                client.consume(usize::MAX);
            }
            if response == RESPONSE {
                // Close first; the server answers with its own close_notify.
                client.close().unwrap();
            }
        }
        assert!(client.transmit().is_empty(), "everything queued was sent");
        assert!(client.is_established());
        assert_eq!(response, RESPONSE);
        assert_eq!(client.session_id().map(<[u8]>::len), Some(32));
        assert_eq!(client.peer_chain().map(|chain| chain.der().len()), Some(1));

        fs::remove_dir_all(&directory).ok();
        (sent, received)
    }

    #[test]
    #[ignore = "needs the openssl binary and a free loopback port"]
    fn client_completes_a_handshake_and_an_exchange_with_openssl() {
        let (sent, received) = exchange("interop", "127.0.0.1:44435", &[]);
        if let Ok(record_directory) = env::var("TLS_RECORD_DIR") {
            fs::create_dir_all(&record_directory).unwrap();
            fs::write(Path::new(&record_directory).join("client.bin"), &sent).unwrap();
            fs::write(Path::new(&record_directory).join("server.bin"), &received).unwrap();
        }
    }

    /// `-verify 1` makes the server send a `CertificateRequest` and accept a client that
    /// has no certificate to give.
    #[test]
    #[ignore = "needs the openssl binary and a free loopback port"]
    fn client_answers_openssls_certificate_request_with_no_certificate() {
        let (_, received) = exchange("interop-request", "127.0.0.1:44436", &["-verify", "1"]);
        // Five handshake records in the server's first flight instead of four: the fourth
        // message, before ServerHelloDone, is the request (type 13).
        let request = received
            .windows(6)
            .any(|bytes| bytes[..3] == [22, 3, 3] && bytes[5] == 13);
        assert!(request, "the server sent a CertificateRequest");
    }
}
