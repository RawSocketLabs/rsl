//! The `mock` feature's in-memory transports — [`MockStream`]/[`MockDatagramSocket`] — driving
//! the `net` wrappers without a real socket. `component` exercises one call at a time (a queued
//! read, a captured write, chunked reassembly, error injection, the accessors); `e2e` runs a
//! full request/response session over the mocks.
#![cfg(feature = "mock")]

use bnb::{MessageDatagram, MessageStream, MockDatagramSocket, MockStream, bin};

#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Msg {
    seq: u16,
}

mod component {
    use super::*;

    // --- MessageStream over MockStream -------------------------------------------------

    #[test]
    fn stream_write_message_is_captured() {
        let mut conn = MessageStream::new(MockStream::new());
        conn.write_message(&Msg { seq: 7 }).unwrap();
        assert_eq!(
            conn.get_mut().written(),
            &Msg { seq: 7 }.to_bytes().unwrap()[..]
        );
    }

    #[test]
    fn stream_reads_a_queued_message() {
        let mut conn = MessageStream::new(MockStream::new());
        conn.get_mut()
            .push_inbound(&Msg { seq: 0xABCD }.to_bytes().unwrap());
        assert_eq!(conn.read_message::<Msg>().unwrap(), Msg { seq: 0xABCD });
    }

    #[test]
    fn stream_reassembles_a_message_split_across_reads() {
        // One byte per read forces the buffer-more-and-retry loop in read_message.
        let mut conn = MessageStream::new(MockStream::with_chunk_size(1));
        conn.get_mut()
            .push_inbound(&Msg { seq: 0x1234 }.to_bytes().unwrap());
        assert_eq!(conn.read_message::<Msg>().unwrap(), Msg { seq: 0x1234 });
    }

    #[test]
    fn stream_eof_mid_message_is_an_error() {
        // Only one of the two needed bytes is available, then the connection closes.
        let mut conn = MessageStream::new(MockStream::new());
        conn.get_mut().push_inbound(&[0x12]);
        assert!(conn.read_message::<Msg>().is_err());
    }

    #[test]
    fn stream_connection_reset_surfaces_as_an_error() {
        let mut conn = MessageStream::new(MockStream::new().fail_after(0));
        conn.get_mut()
            .push_inbound(&Msg { seq: 1 }.to_bytes().unwrap());
        assert!(conn.read_message::<Msg>().is_err());
    }

    #[test]
    fn stream_into_inner_recovers_the_transport() {
        let conn = MessageStream::new(MockStream::new());
        let _inner: MockStream = conn.into_inner();
    }

    // --- MessageDatagram over MockDatagramSocket ---------------------------------------

    #[test]
    fn datagram_recv_then_send_to_the_sender() {
        let mut peer = MessageDatagram::new(MockDatagramSocket::new());
        let from = "127.0.0.1:5000".parse().unwrap();
        peer.get_ref()
            .push_inbound(&Msg { seq: 7 }.to_bytes().unwrap(), from);

        let (msg, who) = peer.recv_message::<Msg>().unwrap();
        assert_eq!(msg, Msg { seq: 7 });
        assert_eq!(who, from);

        let n = peer.send_message(&Msg { seq: 8 }, &who).unwrap();
        assert_eq!(n, 2, "send_message returns the byte count");
        assert_eq!(
            peer.get_ref().sent()[0].0,
            Msg { seq: 8 }.to_bytes().unwrap()
        );
        assert_eq!(
            peer.get_ref().sent()[0].1,
            who,
            "sent to the original sender"
        );
    }

    #[test]
    fn datagram_recv_error_is_injected() {
        let mut peer = MessageDatagram::new(MockDatagramSocket::new().fail_next_recv());
        assert!(peer.recv_message::<Msg>().is_err());
    }

    #[test]
    fn datagram_recv_malformed_is_a_codec_error() {
        #[bin(big, magic = 0xCAFEu16)]
        #[derive(Debug, PartialEq, Eq)]
        struct M {
            v: u8,
        }
        let mut peer = MessageDatagram::new(MockDatagramSocket::new());
        let from = "127.0.0.1:1".parse().unwrap();
        peer.get_ref().push_inbound(&[0x00, 0x00, 0x09], from); // wrong magic
        assert!(peer.recv_message::<M>().is_err());
    }

    #[test]
    fn datagram_with_capacity_truncates_an_oversized_datagram() {
        // Capacity 2 → only the first two bytes are delivered (OS-style truncation).
        let mut peer = MessageDatagram::with_capacity(MockDatagramSocket::new(), 2);
        let from = "127.0.0.1:2".parse().unwrap();
        peer.get_ref().push_inbound(&[0x00, 0x05, 0xFF, 0xFF], from);
        let (msg, _) = peer.recv_message::<Msg>().unwrap();
        assert_eq!(msg, Msg { seq: 5 });
    }

    #[test]
    fn datagram_get_mut_and_into_inner() {
        let mut peer = MessageDatagram::new(MockDatagramSocket::new());
        let _m: &mut MockDatagramSocket = peer.get_mut();
        let _inner: MockDatagramSocket = peer.into_inner();
    }
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
