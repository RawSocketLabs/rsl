//! **tokio_framed** — [`bnb::BinCodec`] (the `tokio` feature) framing `#[bin]` messages over an
//! async `Framed` TCP stream, against a real client/server.
//!
//! `BinCodec<T>` is `tokio_util`'s `Decoder`/`Encoder` for *any* `#[bin]` message, so
//! `Framed::new(stream, BinCodec::<T>::new())` is a `Stream + Sink` of `T` — no per-type glue.
//! Here a server answers Pings with Pongs until Bye, all async over loopback.
//!
//! Run with: `cargo run -p bitsandbytes --example tokio_framed --features tokio`

use bnb::{BinCodec, bin};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::Framed;
use tracing::info;

/// A tiny self-delimiting RPC (magic-dispatched), the message type the codec frames.
#[bin(big, magic = b"RPC")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {
    #[bin(magic = 0x01u8)]
    Ping { seq: u32 },
    #[bin(magic = 0x02u8)]
    Pong { seq: u32 },
    #[bin(magic = 0x03u8)]
    Bye,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    // Server task: wrap the accepted socket in `Framed` and answer Pings until Bye.
    let server = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.expect("accept");
        let mut framed = Framed::new(sock, BinCodec::<Message>::new());
        while let Some(msg) = framed.next().await {
            let req = msg.expect("decode request");
            info!(?req, "server ← request");
            match req {
                Message::Ping { seq } => framed.send(Message::Pong { seq }).await.expect("send"),
                Message::Bye => break,
                Message::Pong { .. } => {}
            }
        }
        info!("server: connection closed");
    });

    // Client: same `Framed` wrapper; send Pings as a `Sink`, read Pongs as a `Stream`.
    let sock = TcpStream::connect(addr).await?;
    let mut framed = Framed::new(sock, BinCodec::<Message>::new());
    for seq in [1u32, 2, 3] {
        framed.send(Message::Ping { seq }).await?;
        info!(seq, "client → Ping");
        let reply = framed.next().await.expect("a reply")?;
        info!(?reply, "client ← reply");
        assert_eq!(reply, Message::Pong { seq });
    }
    framed.send(Message::Bye).await?;
    info!("client → Bye");

    server.await?;
    info!("all checks passed");
    Ok(())
}
