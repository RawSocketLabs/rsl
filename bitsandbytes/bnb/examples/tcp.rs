//! **TCP streaming** — a real client/server over a reliable byte stream, reading and writing
//! framed messages on one connection, *without* `try_clone()`.
//!
//! A `#[bin]` enum is a self-delimiting RPC (magic-dispatched), so a reader pulls exactly one
//! message off the stream at a time. The read side wraps the socket in a [`BufSource`] (the
//! `std::io::Read` → bnb `Source` adapter that buffers as the decoder needs bytes, so
//! `decode_from` blocks until a whole message has arrived). The write side just calls
//! `to_bytes()` + `write_all`.
//!
//! **The duplex trick:** `std`'s `&TcpStream` implements *both* `Read` and `Write`, so the read
//! half (a `BufSource<&TcpStream>`) and the write half (`&TcpStream`) are two shared borrows of
//! the *same* socket — no `try_clone()` (which dups the fd), no ownership split. For halves you
//! need to *move* across threads, the equivalent is `Arc<TcpStream>` (what tokio's `into_split`
//! does under the hood).
//!
//! Run with: `cargo run -p bitsandbytes --example tcp`

use bnb::{BufSource, bin};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::thread;
use tracing::info;

/// A tiny self-delimiting RPC: a `b"RPC"` sync prefix, then a one-byte variant `magic`.
#[bin(big, magic = b"RPC")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {
    #[bin(magic = 0x01u8)]
    Ping { seq: u32 },
    #[bin(magic = 0x02u8)]
    Pong { seq: u32 },
    #[bin(magic = 0x03u8)]
    Echo {
        #[br(temp)]
        #[bw(calc = text.len() as u8)]
        len: u8,
        #[br(count = len)]
        text: Vec<u8>,
    },
    #[bin(magic = 0x04u8)]
    Bye,
}

fn echo(text: &str) -> Message {
    Message::Echo {
        text: text.as_bytes().to_vec(),
    }
}

/// The server's reply to one request (`None` = close the connection).
fn reply_to(req: &Message) -> Option<Message> {
    match req {
        Message::Ping { seq } => Some(Message::Pong { seq: *seq }),
        Message::Echo { text } => Some(Message::Echo { text: text.clone() }), // echo it back
        Message::Bye => None,                                                 // hang up
        Message::Pong { .. } => None,                                         // unexpected here
    }
}

/// Serve one connection: read framed requests off the stream and write replies, on the *same*
/// socket — read half is a `BufSource<&TcpStream>`, write half is `&TcpStream` (no `try_clone`).
fn serve(stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufSource::new(&stream); // &TcpStream: Read
    let mut writer = &stream; // &TcpStream: Write — the same underlying socket
    // `decode_from` returns `Err` when the connection closes (or on a framing error) — that
    // ends the read loop.
    while let Ok(req) = Message::decode_from(&mut reader) {
        info!(?req, "server ← request");
        match reply_to(&req) {
            Some(reply) => {
                writer.write_all(&reply.to_bytes().expect("encode reply"))?;
                info!(?reply, "server → reply");
            }
            None => {
                info!("server: closing connection");
                break;
            }
        }
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    // A server on an ephemeral loopback port, handling exactly one connection in a thread.
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let server = thread::spawn(move || {
        let (stream, peer) = listener.accept().expect("accept");
        info!(%peer, "server: connection accepted");
        serve(stream).expect("serve");
    });

    // The client: one connection, write requests and read the streamed replies — same duplex
    // trick (a `BufSource<&TcpStream>` to read, `&TcpStream` to write; no `try_clone`).
    let stream = TcpStream::connect(addr)?;
    let mut reader = BufSource::new(&stream);
    let mut writer = &stream;

    for request in [Message::Ping { seq: 1 }, echo("hello"), echo("world")] {
        let bytes = request.to_bytes()?;
        writer.write_all(&bytes)?;
        info!(?request, bytes = %hex(&bytes), "client → request");

        let reply = Message::decode_from(&mut reader)?; // one framed reply off the stream
        info!(?reply, "client ← reply");
        match (&request, &reply) {
            (Message::Ping { seq }, Message::Pong { seq: back }) => assert_eq!(seq, back),
            (Message::Echo { text }, Message::Echo { text: back }) => assert_eq!(text, back),
            _ => panic!("unexpected reply {reply:?}"),
        }
    }

    // Say goodbye, which tells the server to close.
    writer.write_all(&Message::Bye.to_bytes()?)?;
    info!("client → Bye");

    server.join().expect("server thread panicked");
    info!("all checks passed");
    Ok(())
}
