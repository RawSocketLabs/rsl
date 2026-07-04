//! End-to-end `tokio` framing: the [`BinCodec`] driven over real async transports — a `Framed`
//! duplex stream and `UdpFramed` datagrams. The synchronous Decoder/Encoder component tests
//! live inline in `src/codec.rs`.
#![cfg(feature = "tokio")]

use bnb::{BinCodec, bin};

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
