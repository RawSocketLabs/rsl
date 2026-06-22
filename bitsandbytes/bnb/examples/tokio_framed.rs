//! **tokio_framed** — *prototype*: a `tokio_util::codec` adapter so any bnb `#[bin]` message
//! flows over an async `Framed` TCP stream, evaluated against a real client/server.
//!
//! The whole adapter is the ~30-line [`BinCodec`] below: `Decoder` reads one message off the
//! receive buffer (returning `None` when only a partial frame is present — `Framed` then waits
//! for more bytes), and `Encoder` appends `bit_encode`'s output. It is **generic over any
//! `BitDecode + BitEncode`** — no per-type glue — so `Framed::new(stream, BinCodec::<T>::new())`
//! gives you a `Stream + Sink` of `T`.
//!
//! This lives in an example (not the crate) on purpose: it's a prototype to judge the ergonomics
//! before deciding whether a `bnb::BinCodec` behind a `tokio` feature earns its place.
//!
//! Run with: `cargo run -p bitsandbytes --example tokio_framed --features bytes`

use bnb::{BitDecode, BitEncode, BitReader, BitWriter, ErrorKind, bin};
use bytes::{Buf, BytesMut};
use futures_util::{SinkExt, StreamExt};
use std::io;
use std::marker::PhantomData;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Decoder, Encoder, Framed};
use tracing::info;

/// A `tokio_util` codec for any bnb `#[bin]` message — the entire bnb↔tokio adapter.
struct BinCodec<T>(PhantomData<T>);

impl<T> BinCodec<T> {
    fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: BitDecode> Decoder for BinCodec<T> {
    type Item = T;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<T>, io::Error> {
        if src.is_empty() {
            return Ok(None);
        }
        let mut reader = BitReader::new(&src[..]);
        match <T as BitDecode>::bit_decode(&mut reader) {
            Ok(item) => {
                // Consume exactly what this message used; leave the rest for the next call.
                let consumed = reader.bit_pos() / 8; // these protocols are byte-aligned
                src.advance(consumed);
                Ok(Some(item))
            }
            // The buffer holds only a partial frame — tell `Framed` to read more (don't consume).
            Err(e)
                if matches!(
                    e.kind,
                    ErrorKind::UnexpectedEof { .. } | ErrorKind::Incomplete { .. }
                ) =>
            {
                Ok(None)
            }
            // A genuine framing error.
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string())),
        }
    }
}

impl<T: BitEncode> Encoder<T> for BinCodec<T> {
    type Error = io::Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), io::Error> {
        let mut w = BitWriter::with_layout(<T as BitEncode>::LAYOUT);
        item.bit_encode(&mut w)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        dst.extend_from_slice(&w.into_bytes());
        Ok(())
    }
}

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
