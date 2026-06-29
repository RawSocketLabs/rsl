//! End-to-end `net` sessions: a full request/response loop over the in-memory mock stream
//! (the `mock` feature), and a datagram exchange over a real loopback `UdpSocket`. The
//! one-call-at-a-time component tests live inline in `src/net.rs`.
#![cfg(feature = "mock")]

use bnb::{MessageDatagram, MessageStream, MockStream, bin};

#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Msg {
    seq: u16,
}

mod e2e {
    use super::*;

    /// A multi-message session over one mock stream: several writes are read back in order,
    /// then EOF ends the loop — the request/response shape a `MessageStream` user writes.
    #[test]
    fn stream_session_exchanges_then_closes() {
        let mut conn = MessageStream::new(MockStream::new());

        // Three messages "sent by the peer" arrive on the inbound side.
        for seq in [1u16, 2, 3] {
            let bytes = Msg { seq }.to_bytes().unwrap();
            conn.get_mut().push_inbound(&bytes);
        }
        for seq in [1u16, 2, 3] {
            assert_eq!(conn.read_message::<Msg>().unwrap(), Msg { seq });
        }
        // Inbound drained → the next read sees EOF.
        assert!(conn.read_message::<Msg>().is_err());
    }

    /// A datagram exchange: two `MessageDatagram`s talk over a pair of real loopback UDP
    /// sockets — the same wrappers, end to end, with no mock in the path.
    #[test]
    fn datagram_session_over_loopback_udp() {
        use std::net::UdpSocket;

        let server = MessageDatagram::new(UdpSocket::bind("127.0.0.1:0").unwrap());
        let server_addr = server.get_ref().local_addr().unwrap();
        let client = MessageDatagram::new(UdpSocket::bind("127.0.0.1:0").unwrap());

        client.send_message(&Msg { seq: 42 }, &server_addr).unwrap();
        let mut server = server;
        let (req, from) = server.recv_message::<Msg>().unwrap();
        assert_eq!(req, Msg { seq: 42 });

        server.send_message(&Msg { seq: 43 }, &from).unwrap();
        let mut client = client;
        let (reply, _) = client.recv_message::<Msg>().unwrap();
        assert_eq!(reply, Msg { seq: 43 });
    }
}
