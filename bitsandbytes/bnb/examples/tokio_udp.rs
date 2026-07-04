//! **tokio_udp** — async UDP datagrams with [`bnb::BinCodec`] and `tokio_util`'s `UdpFramed`:
//! the datagram analog of `tokio_framed` (which uses `Framed` over a TCP stream).
//!
//! The same `BinCodec<T>` drives **both** transports — `Framed` for a TCP stream, `UdpFramed`
//! for a UDP socket — because it's just a `tokio_util` `Decoder`/`Encoder`. `UdpFramed` is a
//! `Stream<Item = (T, SocketAddr)>` + `Sink<(T, SocketAddr)>`, so every message carries its
//! peer address (the datagram nature). This mirrors the sync split `MessageStream` (net) vs
//! `MessageDatagram` (net) — bnb supplies the codec; tokio_util supplies the framing.
//!
//! Run with: `cargo run -p bitsandbytes --example tokio_udp --features tokio`

use bnb::{BinCodec, bin};
use futures_util::{SinkExt, StreamExt};
use tokio::net::UdpSocket;
use tokio_util::udp::UdpFramed;
use tracing::info;

#[bin(big, magic = b"RPC")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {
    #[bin(magic = 0x01u8)]
    Ping { seq: u32 },
    #[bin(magic = 0x02u8)]
    Pong { seq: u32 },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    let server_sock = UdpSocket::bind("127.0.0.1:0").await?;
    let server_addr = server_sock.local_addr()?;
    let client_sock = UdpSocket::bind("127.0.0.1:0").await?;

    // Each socket framed with the *same* BinCodec — now a Stream + Sink of (Message, addr).
    let mut server = UdpFramed::new(server_sock, BinCodec::<Message>::new());
    let mut client = UdpFramed::new(client_sock, BinCodec::<Message>::new());

    for seq in [1u32, 2, 3] {
        // A datagram carries its destination; the reply carries the sender it came from.
        client.send((Message::Ping { seq }, server_addr)).await?;
        info!(seq, "client → Ping");

        let (req, from) = server.next().await.expect("a datagram")?;
        info!(?req, %from, "server ← request");
        if let Message::Ping { seq } = req {
            server.send((Message::Pong { seq }, from)).await?;
        }

        let (reply, _) = client.next().await.expect("a datagram")?;
        info!(?reply, "client ← reply");
        assert_eq!(reply, Message::Pong { seq });
    }

    info!("all checks passed");
    Ok(())
}
