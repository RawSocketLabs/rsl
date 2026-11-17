//! The network layer against a real TLS server on loopback: `openssl s_server`.
//!
//! Ignored by default because it needs the `openssl` binary and free loopback ports. Run it
//! with `cargo test -p sangfor-atrust --features net --test interop -- --ignored`. It checks
//! what the scripted tests cannot: that the socket, TLS and HTTP layers fit together under
//! `HttpsPortal`, and that a TLS connection is the `Transport` a tunnel needs. No gateway is
//! involved; nothing here speaks the aTrust protocol to a peer that understands it.

#[path = "support/mod.rs"]
mod support;

mod interop {
    use std::{
        env, fs,
        io::{ErrorKind, Read, Write},
        net::TcpStream,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        thread,
        time::Duration,
    };

    use http1::Limits;
    use sangfor_atrust::api::{Call, Context, Gateway, Portal};
    use sangfor_atrust::net::{HttpsPortal, dial};
    use sangfor_atrust::tcp::Transport;

    use super::support::Counting;

    const TIMEOUT: Duration = Duration::from_secs(10);

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

    /// Starts `openssl s_server` on `address` with the flag `behaviour` and a fresh throwaway certificate,
    /// and waits until it accepts connections. Returns the server and its directory.
    fn start_server(name: &str, address: &str, behaviour: &str) -> (Server, PathBuf) {
        let directory = env::temp_dir().join(format!("atrust-{name}-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let made = openssl(
            &directory,
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

        let mut portal = HttpsPortal::new(&gateway, Counting::default()).with_timeout(TIMEOUT);
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
        let mut node = dial("127.0.0.1:44492", TIMEOUT, &mut Counting::default()).unwrap();

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
