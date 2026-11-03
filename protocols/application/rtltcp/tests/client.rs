//! The blocking client end to end over real loopback TCP, against a scripted server thread.

mod client {
    use std::io::{ErrorKind, Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use bnb::{ErrorKind as CodecErrorKind, StreamBitReader};
    use rtltcp::{Client, ClientError, Command, Greeting, Opcode};

    const TIMEOUT: Duration = Duration::from_secs(2);

    /// A scripted server: sends `hello` then `payload`. With `hold_open` it keeps the socket
    /// open until the client hangs up and returns every command it received; without, it
    /// closes at once, which the client sees as the server going away.
    fn spawn_server(
        hello: Vec<u8>,
        payload: Vec<u8>,
        hold_open: bool,
    ) -> (String, JoinHandle<Vec<Command>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let handle = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            sock.write_all(&hello).unwrap();
            sock.write_all(&payload).unwrap();
            if !hold_open {
                return Vec::new();
            }
            let mut received = Vec::new();
            sock.read_to_end(&mut received).unwrap();
            Command::decode_all(&received).unwrap()
        });
        (addr, handle)
    }

    fn hello() -> Vec<u8> {
        Greeting::new(1, 29).to_bytes().unwrap()
    }

    /// Hang up, then collect the commands the server received.
    fn shutdown(client: Client, server: JoinHandle<Vec<Command>>) -> Vec<Command> {
        drop(client);
        server.join().unwrap()
    }

    #[test]
    fn connect_reads_the_greeting_and_disables_nagle() {
        let (addr, server) = spawn_server(hello(), Vec::new(), true);
        let client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        assert_eq!(client.greeting(), Greeting::new(1, 29));
        assert!(client.get_ref().nodelay().unwrap());
        shutdown(client, server);
    }

    #[test]
    fn a_server_without_rtl0_magic_is_refused() {
        let (addr, server) = spawn_server(b"NOPE\0\0\0\0\0\0\0\0".to_vec(), Vec::new(), false);
        let err = Client::connect(addr.as_str(), TIMEOUT).unwrap_err();
        assert!(
            matches!(&err, ClientError::Greeting(e) if matches!(e.kind, CodecErrorKind::BadMagic { .. })),
            "{err}"
        );
        server.join().unwrap();
    }

    #[test]
    fn a_short_greeting_is_a_handshake_error() {
        let (addr, server) = spawn_server(b"RTL0\0\0".to_vec(), Vec::new(), false);
        let err = Client::connect(addr.as_str(), TIMEOUT).unwrap_err();
        assert!(matches!(err, ClientError::Greeting(_)), "{err}");
        server.join().unwrap();
    }

    #[test]
    fn a_refused_connection_is_a_connect_error() {
        let addr = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let err = Client::connect(addr, TIMEOUT).unwrap_err();
        assert!(matches!(err, ClientError::Io(_)), "{err}");
    }

    #[test]
    fn commands_reach_the_server_in_order() {
        let (addr, server) = spawn_server(hello(), Vec::new(), true);
        let mut client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        client.set_sample_rate(2_048_000).unwrap();
        client.set_gain_mode(true).unwrap();
        client.set_tuner_gain_by_index(28).unwrap();
        client.send(Command::new(Opcode::Other(0x7f), 7)).unwrap();
        assert_eq!(
            shutdown(client, server),
            [
                Command::set_sample_rate(2_048_000),
                Command::set_gain_mode(true),
                Command::set_tuner_gain_by_index(28),
                Command::new(Opcode::Other(0x7f), 7),
            ]
        );
    }

    #[test]
    fn read_exact_returns_stream_bytes() {
        let (addr, server) = spawn_server(hello(), vec![0, 255, 128, 127], true);
        let mut client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        let mut buf = [0u8; 4];
        client.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [0, 255, 128, 127]);
        shutdown(client, server);
    }

    #[test]
    fn a_truncated_stream_is_closed() {
        let (addr, server) = spawn_server(hello(), vec![1, 2], false);
        let mut client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        let mut buf = [0u8; 8];
        let err = client.read_exact(&mut buf).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof, "{err}");
        shutdown(client, server);
    }

    #[test]
    fn discard_skips_exactly_the_requested_bytes() {
        let (addr, server) = spawn_server(hello(), vec![1, 2, 3, 4, 5, 6], true);
        let mut client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        client.discard(4).unwrap();
        let mut buf = [0u8; 2];
        client.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [5, 6]);
        shutdown(client, server);
    }

    #[test]
    fn drain_drops_buffered_bytes_and_restores_blocking() {
        // The server sends a backlog, then answers one command with a marker, so the client
        // can tell the drained backlog from what arrived after it.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            sock.write_all(&hello()).unwrap();
            sock.write_all(&[0xaa; 1024]).unwrap();
            Command::decode(&mut StreamBitReader::new(&sock)).unwrap();
            sock.write_all(&[0x55]).unwrap();
            let mut sink = [0u8; 64];
            while matches!(sock.read(&mut sink), Ok(n) if n > 0) {}
        });
        let mut client = Client::connect(addr, TIMEOUT).unwrap();
        // Wait until the whole backlog is readable before draining it.
        let mut peek = [0u8; 1024];
        while client.get_ref().peek(&mut peek).unwrap() < peek.len() {
            thread::sleep(Duration::from_millis(5));
        }
        client.drain_buffered().unwrap();
        client.set_center_frequency(100_000_000).unwrap();
        let mut marker = [0u8; 1];
        client.read_exact(&mut marker).unwrap();
        assert_eq!(marker, [0x55]);
        drop(client);
        server.join().unwrap();
    }

    #[test]
    fn drain_reports_a_closed_server() {
        let (addr, server) = spawn_server(hello(), Vec::new(), false);
        let mut client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        server.join().unwrap();
        // A blocking peek returns 0 once the FIN has arrived, so the non-blocking drain below
        // sees EOF rather than WouldBlock.
        let mut peek = [0u8; 1];
        assert_eq!(client.get_ref().peek(&mut peek).unwrap(), 0);
        let err = client.drain_buffered().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof, "{err}");
    }
}
