//! **unix_stream** — `MessageStream` over a Unix-domain **stream** socket (not TCP). The wrapper
//! is generic over any `Read + Write`, so `MessageStream<UnixStream>` is a request/response
//! connection over a filesystem socket with the *same* `read_message`/`write_message` API as the
//! TCP case in `sockets`. (The stream counterpart to that example's Unix *datagram* demo.)
//!
//! Run with: `cargo run -p bitsandbytes --example unix_stream --features net`

#[cfg(unix)]
use bnb::bin;

/// A tiny echo RPC.
#[cfg(unix)]
#[bin(big, magic = b"RPC")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {
    #[bin(magic = 0x01u8)]
    Echo {
        #[br(temp)]
        #[bw(calc = text.len() as u8)]
        len: u8,
        #[br(count = len)]
        #[try_str]
        text: Vec<u8>,
    },
    #[bin(magic = 0x02u8)]
    Bye,
}

#[cfg(unix)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use bnb::MessageStream;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::thread;
    use tracing::info;

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    let path = std::env::temp_dir().join(format!("bnb-unix-stream-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut conn = MessageStream::new(stream); // MessageStream<UnixStream> — same API as TCP
        while let Ok(req) = conn.read_message::<Message>() {
            info!(?req, "unix server ← request");
            match req {
                Message::Echo { text } => {
                    conn.write_message(&Message::Echo { text }).expect("write")
                }
                Message::Bye => break,
            }
        }
    });

    let mut conn = MessageStream::new(UnixStream::connect(&path)?);
    conn.write_message(&Message::Echo {
        text: b"hello over a unix socket".to_vec(),
    })?;
    let reply = conn.read_message::<Message>()?;
    info!(?reply, "unix client ← reply");
    assert!(matches!(reply, Message::Echo { .. }));
    conn.write_message(&Message::Bye)?;

    server.join().expect("server thread");
    let _ = std::fs::remove_file(&path);
    info!("all checks passed");
    Ok(())
}

#[cfg(not(unix))]
fn main() {
    println!("unix_stream requires a Unix platform; skipping");
}
