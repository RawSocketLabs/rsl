//! The blocking client end to end over real loopback TCP, against a scripted server thread.

mod client {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use rtltcp::{Client, ClientError, Command, Greeting, Opcode};

    const TIMEOUT: Duration = Duration::from_secs(2);

    /// A scripted server: sends `hello` then `payload`, collects `expect_cmds` commands and
    /// publishes them. With `hold_open` it then waits for the client to hang up; without, it
    /// closes, which the client sees as the server going away.
    fn spawn_server(
        hello: Vec<u8>,
        payload: Vec<u8>,
        expect_cmds: usize,
        hold_open: bool,
    ) -> (String, Receiver<Vec<[u8; Command::LEN]>>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            sock.write_all(&hello).unwrap();
            sock.write_all(&payload).unwrap();
            let mut cmds = Vec::with_capacity(expect_cmds);
            for _ in 0..expect_cmds {
                let mut frame = [0u8; Command::LEN];
                if sock.read_exact(&mut frame).is_err() {
                    break;
                }
                cmds.push(frame);
            }
            let _ = tx.send(cmds);
            if hold_open {
                let mut sink = [0u8; 64];
                while matches!(sock.read(&mut sink), Ok(n) if n > 0) {}
            }
        });
        (addr, rx, handle)
    }

    fn hello() -> Vec<u8> {
        Greeting::new(1, 29).to_bytes().unwrap()
    }

    fn shutdown(client: Client, server: JoinHandle<()>) {
        drop(client);
        server.join().unwrap();
    }

    #[test]
    fn connect_reads_the_greeting_and_disables_nagle() {
        let (addr, _rx, server) = spawn_server(hello(), Vec::new(), 0, true);
        let client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        assert_eq!(client.greeting(), Greeting::new(1, 29));
        assert!(client.get_ref().nodelay().unwrap());
        shutdown(client, server);
    }

    #[test]
    fn a_server_without_rtl0_magic_is_refused() {
        let (addr, _rx, server) =
            spawn_server(b"NOPE\0\0\0\0\0\0\0\0".to_vec(), Vec::new(), 0, false);
        let err = Client::connect(addr.as_str(), TIMEOUT).unwrap_err();
        assert!(
            matches!(err, ClientError::NotRtlTcp { magic } if &magic == b"NOPE"),
            "{err}"
        );
        server.join().unwrap();
    }

    #[test]
    fn a_short_greeting_is_a_handshake_error() {
        let (addr, _rx, server) = spawn_server(b"RTL0\0\0".to_vec(), Vec::new(), 0, false);
        let err = Client::connect(addr.as_str(), TIMEOUT).unwrap_err();
        assert!(matches!(err, ClientError::Handshake(_)), "{err}");
        server.join().unwrap();
    }

    #[test]
    fn a_refused_connection_is_a_connect_error() {
        let addr = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let err = Client::connect(addr, TIMEOUT).unwrap_err();
        assert!(matches!(err, ClientError::Connect(_)), "{err}");
    }

    #[test]
    fn commands_reach_the_server_in_order() {
        let (addr, rx, server) = spawn_server(hello(), Vec::new(), 4, true);
        let mut client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        client.set_sample_rate(2_048_000).unwrap();
        client.set_gain_mode(true).unwrap();
        client.set_tuner_gain_by_index(28).unwrap();
        client.send(Command::new(Opcode::Other(0x7f), 7)).unwrap();
        let cmds = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        shutdown(client, server);
        assert_eq!(
            cmds,
            [
                [0x02, 0x00, 0x1f, 0x40, 0x00],
                [0x03, 0x00, 0x00, 0x00, 0x01],
                [0x0d, 0x00, 0x00, 0x00, 0x1c],
                [0x7f, 0x00, 0x00, 0x00, 0x07],
            ]
        );
    }

    #[test]
    fn read_exact_returns_stream_bytes() {
        let (addr, _rx, server) = spawn_server(hello(), vec![0, 255, 128, 127], 0, true);
        let mut client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        let mut buf = [0u8; 4];
        client.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [0, 255, 128, 127]);
        shutdown(client, server);
    }

    #[test]
    fn a_truncated_stream_is_closed() {
        let (addr, _rx, server) = spawn_server(hello(), vec![1, 2], 0, false);
        let mut client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        let mut buf = [0u8; 8];
        let err = client.read_exact(&mut buf).unwrap_err();
        assert!(matches!(err, ClientError::Closed), "{err}");
        shutdown(client, server);
    }

    #[test]
    fn discard_skips_exactly_the_requested_bytes() {
        let (addr, _rx, server) = spawn_server(hello(), vec![1, 2, 3, 4, 5, 6], 0, true);
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
            let mut frame = [0u8; Command::LEN];
            sock.read_exact(&mut frame).unwrap();
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
        shutdown(client, server);
    }

    #[test]
    fn drain_reports_a_closed_server() {
        let (addr, _rx, server) = spawn_server(hello(), Vec::new(), 0, false);
        let mut client = Client::connect(addr.as_str(), TIMEOUT).unwrap();
        server.join().unwrap();
        // A blocking peek returns 0 once the FIN has arrived, so the non-blocking drain below
        // sees EOF rather than WouldBlock.
        let mut peek = [0u8; 1];
        assert_eq!(client.get_ref().peek(&mut peek).unwrap(), 0);
        let err = client.drain_buffered().unwrap_err();
        assert!(matches!(err, ClientError::Closed), "{err}");
    }
}
