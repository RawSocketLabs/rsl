//! The `tokio` feature's [`BinCodec`] — a `tokio_util` `Decoder`/`Encoder` for any `#[bin]`
//! message. `component` exercises the codec's framing logic synchronously over a `BytesMut`
//! (one message per `decode`, partial-frame `None`, exact consumption, error mapping); `e2e`
//! drives it over real async transports (`Framed` duplex stream + `UdpFramed` datagrams).
#![cfg(feature = "tokio")]

use bnb::{BinCodec, bin};

/// A fixed 4-byte message — its length is implicit in its `#[bin]` structure.
#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Msg {
    a: u16,
    b: u16,
}

/// A magic-prefixed message — wrong bytes make `decode` a hard error, not "read more".
#[bin(big, magic = 0xCAFEu16)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Magic {
    v: u8,
}

mod component {
    use super::*;
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    #[test]
    fn encoder_writes_exact_message_bytes() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::new();
        codec
            .encode(
                Msg {
                    a: 0x0102,
                    b: 0x0304,
                },
                &mut buf,
            )
            .unwrap();
        assert_eq!(&buf[..], &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn encode_then_decode_round_trips_and_drains() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::new();
        let m = Msg {
            a: 0xAABB,
            b: 0xCCDD,
        };
        codec.encode(m.clone(), &mut buf).unwrap();
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(m));
        assert!(buf.is_empty(), "decode consumed exactly the one frame");
    }

    #[test]
    fn decode_empty_buffer_is_none() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::new();
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
    }

    #[test]
    fn decode_partial_frame_is_none_and_keeps_bytes() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::from(&[0x01, 0x02][..]); // only 2 of the 4 bytes
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        assert_eq!(
            &buf[..],
            &[0x01, 0x02],
            "a partial frame is left for the next read"
        );
    }

    #[test]
    fn decode_consumes_one_message_and_leaves_the_tail() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::from(&[0x01, 0x02, 0x03, 0x04, 0xEE, 0xFF][..]);
        assert_eq!(
            codec.decode(&mut buf).unwrap(),
            Some(Msg {
                a: 0x0102,
                b: 0x0304
            })
        );
        assert_eq!(
            &buf[..],
            &[0xEE, 0xFF],
            "trailing bytes remain for the next frame"
        );
    }

    #[test]
    fn decode_walks_back_to_back_messages() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::from(&[0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04][..]);
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(Msg { a: 1, b: 2 }));
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(Msg { a: 3, b: 4 }));
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
    }

    #[test]
    fn decode_bad_magic_is_an_invalid_data_error() {
        let mut codec = BinCodec::<Magic>::new();
        let mut buf = BytesMut::from(&[0x00, 0x00, 0x07][..]); // full frame, wrong magic
        let err = codec.decode(&mut buf).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn default_constructs_a_codec() {
        let _c: BinCodec<Msg> = BinCodec::default();
    }
}

mod e2e {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio_util::codec::Framed;

    /// A self-delimiting magic-dispatched RPC — the message type the codec frames.
    #[bin(big, magic = b"RPC")]
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Rpc {
        #[bin(magic = 0x01u8)]
        Ping { seq: u32 },
        #[bin(magic = 0x02u8)]
        Pong { seq: u32 },
        #[bin(magic = 0x03u8)]
        Bye,
    }

    #[tokio::test]
    async fn framed_stream_request_reply() {
        // An in-memory full-duplex pipe — both ends are `AsyncRead + AsyncWrite`.
        let (a, b) = tokio::io::duplex(1024);
        let mut server = Framed::new(a, BinCodec::<Rpc>::new());
        let mut client = Framed::new(b, BinCodec::<Rpc>::new());

        client.send(Rpc::Ping { seq: 1 }).await.unwrap();
        let req = server.next().await.unwrap().unwrap();
        assert_eq!(req, Rpc::Ping { seq: 1 });

        server.send(Rpc::Pong { seq: 1 }).await.unwrap();
        let reply = client.next().await.unwrap().unwrap();
        assert_eq!(reply, Rpc::Pong { seq: 1 });

        // A no-payload variant frames and round-trips too.
        client.send(Rpc::Bye).await.unwrap();
        assert_eq!(server.next().await.unwrap().unwrap(), Rpc::Bye);
    }

    #[tokio::test]
    async fn udp_framed_datagram_round_trips() {
        use tokio::net::UdpSocket;
        use tokio_util::udp::UdpFramed;

        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server.local_addr().unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client.local_addr().unwrap();

        let mut sf = UdpFramed::new(server, BinCodec::<Rpc>::new());
        let mut cf = UdpFramed::new(client, BinCodec::<Rpc>::new());

        cf.send((Rpc::Ping { seq: 5 }, server_addr)).await.unwrap();
        let (req, from) = sf.next().await.unwrap().unwrap();
        assert_eq!(req, Rpc::Ping { seq: 5 });
        assert_eq!(from, client_addr);

        sf.send((Rpc::Pong { seq: 5 }, from)).await.unwrap();
        let (reply, _) = cf.next().await.unwrap().unwrap();
        assert_eq!(reply, Rpc::Pong { seq: 5 });
    }
}
