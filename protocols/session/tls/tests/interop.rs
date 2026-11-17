//! Live interoperability: the blocking stream against a real `openssl s_server` on loopback.
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
        io::{self, Read, Write},
        net::TcpStream,
        path::Path,
        process::{Child, Command, Stdio},
        thread,
        time::Duration,
    };

    use tls::{ClientConfig, ServerVerification, blocking::Stream};

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

    /// A socket that keeps a copy of everything sent and received.
    struct Recorder {
        socket: TcpStream,
        sent: Vec<u8>,
        received: Vec<u8>,
    }

    impl Read for Recorder {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = self.socket.read(buffer)?;
            self.received.extend(&buffer[..count]);
            Ok(count)
        }
    }

    impl Write for Recorder {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            let count = self.socket.write(buffer)?;
            self.sent.extend(&buffer[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.socket.flush()
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
            if let Ok(socket) = TcpStream::connect(address) {
                return (server, socket);
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("openssl s_server did not start listening on {address}");
    }

    /// Connects to a fresh server, sends one line, expects it back reversed, and closes.
    /// Returns every byte sent and received.
    fn exchange(name: &str, address: &str, extra: &[&str]) -> (Vec<u8>, Vec<u8>) {
        let directory = env::temp_dir().join(format!("tls-{name}-{}", std::process::id()));
        let (_server, socket) = start_server(&directory, address, extra);
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let transport = Recorder {
            socket,
            sent: Vec::new(),
            received: Vec::new(),
        };
        let config = ClientConfig {
            server_name: Some("tls.test"),
            verification: ServerVerification::Unverified,
        };

        // The reproducible source, so that the bytes this records can be replayed.
        let mut random = CountingRandom::default();
        let mut stream = Stream::connect(transport, config, &mut random).unwrap();
        assert_eq!(stream.client().session_id().map(<[u8]>::len), Some(32));
        let chain = stream.client().peer_chain().unwrap();
        assert_eq!(chain.der().len(), 1);

        stream.write_all(REQUEST).unwrap();
        let mut response = [0; 10];
        stream.read_exact(&mut response).unwrap();
        assert_eq!(response, RESPONSE);

        // Close first; the server answers with its own close_notify.
        stream.close().unwrap();
        assert_eq!(stream.read(&mut response).unwrap(), 0);

        fs::remove_dir_all(&directory).ok();
        let transport = stream.get_ref();
        (transport.sent.clone(), transport.received.clone())
    }

    #[test]
    #[ignore = "needs the openssl binary and a free loopback port"]
    fn stream_completes_a_handshake_and_an_exchange_with_openssl() {
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
    fn stream_answers_openssls_certificate_request_with_no_certificate() {
        let extra = ["-verify", "1"];
        let (_, received) = exchange("interop-request", "127.0.0.1:44436", &extra);
        // Five handshake records in the server's first flight instead of four: the fourth
        // message, before ServerHelloDone, is the request (type 13).
        let request = received
            .windows(6)
            .any(|bytes| bytes[..3] == [22, 3, 3] && bytes[5] == 13);
        assert!(request, "the server sent a CertificateRequest");
    }
}
