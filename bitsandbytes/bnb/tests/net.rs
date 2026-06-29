//! The `net` feature: `MessageStream` (whole-message read/write over a `Read + Write`) and
//! `MessageDatagram` (datagram send/recv over any `DatagramSocket`).
#![cfg(feature = "net")]

mod e2e {

    use bnb::{MessageDatagram, MessageStream, bin};
    use std::io::Read;
    use std::net::UdpSocket;
    use std::time::Duration;

    #[bin(big, magic = b"M")]
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Msg {
        #[bin(magic = 0x01u8)]
        Hi { n: u16 },
        #[bin(magic = 0x02u8)]
        Bye,
    }

    #[test]
    fn message_stream_round_trips_multiple_messages() {
        // Write three messages into a `Vec` (which is `Write`), then read them back from the
        // bytes (a `&[u8]` is `Read`) one at a time — the cursor advances per message.
        let mut out = MessageStream::new(Vec::new());
        out.write_message(&Msg::Hi { n: 1 }).unwrap();
        out.write_message(&Msg::Hi { n: 2 }).unwrap();
        out.write_message(&Msg::Bye).unwrap();
        let bytes = out.into_inner();

        let mut inp = MessageStream::new(&bytes[..]);
        assert_eq!(inp.read_message::<Msg>().unwrap(), Msg::Hi { n: 1 });
        assert_eq!(inp.read_message::<Msg>().unwrap(), Msg::Hi { n: 2 });
        assert_eq!(inp.read_message::<Msg>().unwrap(), Msg::Bye);
        // The stream is drained — the next read reports the closed-connection EOF.
        assert!(inp.read_message::<Msg>().is_err());
    }

    /// A `Read` that yields at most `chunk` bytes per call — to exercise partial-frame buffering.
    struct Trickle<'a> {
        data: &'a [u8],
        chunk: usize,
    }
    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.data.len().min(self.chunk).min(buf.len());
            buf[..n].copy_from_slice(&self.data[..n]);
            self.data = &self.data[n..];
            Ok(n)
        }
    }

    #[test]
    fn message_stream_reassembles_across_partial_reads() {
        let mut out = MessageStream::new(Vec::new());
        out.write_message(&Msg::Hi { n: 0xABCD }).unwrap();
        out.write_message(&Msg::Bye).unwrap();
        let bytes = out.into_inner();

        // Deliver one byte per `read` — `MessageStream` must buffer until each message completes.
        let mut inp = MessageStream::new(Trickle {
            data: &bytes,
            chunk: 1,
        });
        assert_eq!(inp.read_message::<Msg>().unwrap(), Msg::Hi { n: 0xABCD });
        assert_eq!(inp.read_message::<Msg>().unwrap(), Msg::Bye);
    }

    #[test]
    fn message_datagram_over_udp() {
        let server_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        server_sock
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let server_addr = server_sock.local_addr().unwrap();
        let mut server = MessageDatagram::new(server_sock);

        let client_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        client_sock
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut client = MessageDatagram::new(client_sock);

        // Client → server (the datagram is buffered by the OS until the server reads it).
        client
            .send_message(&Msg::Hi { n: 7 }, &server_addr)
            .unwrap();
        let (req, from) = server.recv_message::<Msg>().unwrap();
        assert_eq!(req, Msg::Hi { n: 7 });

        // Server → client (replying to the address the request came from).
        server.send_message(&Msg::Bye, &from).unwrap();
        let (reply, _) = client.recv_message::<Msg>().unwrap();
        assert_eq!(reply, Msg::Bye);
    }

    // Proves `MessageDatagram` is generic over the transport, not just `UdpSocket`: the same code
    // drives a Unix-domain datagram socket.
    #[cfg(unix)]
    #[test]
    fn message_datagram_over_unix() {
        use std::os::unix::net::UnixDatagram;

        let dir = std::env::temp_dir();
        let spath = dir.join(format!("bnb-net-srv-{}.sock", std::process::id()));
        let cpath = dir.join(format!("bnb-net-cli-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&spath);
        let _ = std::fs::remove_file(&cpath);

        let server_sock = UnixDatagram::bind(&spath).unwrap();
        let server_addr = server_sock.local_addr().unwrap();
        let mut server = MessageDatagram::new(server_sock);
        let client = MessageDatagram::new(UnixDatagram::bind(&cpath).unwrap());

        client
            .send_message(&Msg::Hi { n: 9 }, &server_addr)
            .unwrap();
        let (req, _from) = server.recv_message::<Msg>().unwrap();
        assert_eq!(req, Msg::Hi { n: 9 });

        let _ = std::fs::remove_file(&spath);
        let _ = std::fs::remove_file(&cpath);
    }
}
