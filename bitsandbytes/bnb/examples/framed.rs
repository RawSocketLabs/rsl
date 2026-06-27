//! **Framed protocol** — a length-delimited, magic-synced message stream, both sides of a
//! conversation, over the opt-in **`bytes`** zero-copy adapters.
//!
//! This is the example that exercises bnb's async-framing path. One `#[bin]` **enum** is a
//! tagged union dispatched by a per-variant `magic` byte, behind a 2-byte frame-sync prefix
//! (`b"BN"`). Messages are encoded into a [`BytesWriter`] and `freeze()`d to a zero-copy
//! `bytes::Bytes` (what you'd hand a `tokio_util` `Framed` sink), and decoded from a
//! [`BytesReader`] (what you'd get from its stream). It also shows two framing essentials:
//! **several frames packed in one buffer**, decoded one at a time (the cursor advances), and
//! the **`Incomplete` "read more" signal** when a frame is split across reads.
//!
//! Run with: `cargo run -p bitsandbytes --example framed --features bytes`

use bnb::{BitEncode, BitReader, BytesReader, BytesWriter, StreamBitReader, bin};
use bytes::Bytes;
use tracing::info;

/// The wire protocol: a 2-byte sync prefix `b"BN"`, then a one-byte variant `magic` that
/// dispatches the payload. A closed set — an unknown tag is a decode error (reject the frame).
#[bin(big, magic = b"BN")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {
    /// Handshake: the protocol version the peer speaks.
    #[bin(magic = 0x01u8)]
    Hello { proto: u8 },
    /// A text line — a `u8` length prefix (derived, never stored) then that many bytes.
    #[bin(magic = 0x02u8)]
    Say {
        #[br(temp)]
        #[bw(calc = text.len() as u8)]
        len: u8,
        #[br(count = len)]
        #[try_str]
        text: Vec<u8>,
    },
    /// End of conversation.
    #[bin(magic = 0x03u8)]
    Bye,
}

/// Encode one message into a zero-copy frame (the `BytesWriter` → `Bytes` path a `tokio`
/// framed codec would use). `bit_encode` is the [`BitEncode`] trait method that writes into
/// any [`Sink`](bnb::Sink) — here a `BytesWriter`.
fn frame(msg: &Message) -> Bytes {
    let mut w = BytesWriter::new();
    msg.bit_encode(&mut w).expect("encode into BytesWriter");
    w.freeze()
}

/// A `Say` carrying a string literal.
fn say(text: &str) -> Message {
    Message::Say {
        text: text.as_bytes().to_vec(),
    }
}

/// The server's reply to one decoded request.
fn server_reply(req: &Message) -> Message {
    match req {
        Message::Hello { proto } => Message::Hello { proto: *proto }, // ack the version
        Message::Say { text } => {
            let echo = String::from_utf8_lossy(text);
            say(&format!("ack: {echo}"))
        }
        Message::Bye => Message::Bye,
    }
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

    // ===== a conversation: client → server → client, each turn a zero-copy frame =====
    let conversation = [Message::Hello { proto: 1 }, say("ping"), Message::Bye];
    for request in &conversation {
        // Client side: encode the request to a Bytes frame and "send" it.
        let on_wire = frame(request);
        info!(?request, bytes = %hex(&on_wire), "client → server");

        // Server side: decode the frame from an owned Bytes (no copy), then reply.
        let mut reader = BytesReader::new(on_wire);
        let decoded = Message::decode(&mut reader)?;
        let reply = server_reply(&decoded);
        let reply_wire = frame(&reply);
        info!(?reply, bytes = %hex(&reply_wire), "server → client");

        // Client side: decode the reply.
        let echoed = Message::decode(&mut BytesReader::new(reply_wire))?;
        assert_eq!(echoed, reply);
    }

    // ===== framing: several frames in one buffer, decoded one at a time =====
    // A real stream delivers back-to-back frames; `decode` reads exactly one and advances
    // the cursor past it, leaving the rest for the next call.
    let mut buf = Vec::new();
    buf.extend_from_slice(&frame(&Message::Hello { proto: 1 }));
    buf.extend_from_slice(&frame(&say("hi")));
    buf.extend_from_slice(&frame(&Message::Bye));
    info!(bytes = %hex(&buf), "three frames packed in one buffer");

    let mut reader = BitReader::new(&buf);
    let mut count = 0;
    while reader.remaining_bits() > 0 {
        let msg = Message::decode(&mut reader)?; // reads one frame, advances the cursor
        info!(
            ?msg,
            remaining_bits = reader.remaining_bits(),
            "decoded one frame off the stream"
        );
        count += 1;
    }
    assert_eq!(count, 3);

    // ===== streaming: a partial frame is `Incomplete`, not a hard error =====
    // When a frame arrives split across reads, decoding the prefix reports `Incomplete` —
    // the "buffer more bytes and retry" signal — distinct from a malformed-frame error.
    let whole = frame(&say("hello world"));
    let truncated = &whole[..whole.len() - 4]; // the text is cut short
    let err = Message::decode(&mut StreamBitReader::new(truncated)).unwrap_err();
    info!(error = %err, incomplete = err.is_incomplete(), "truncated frame → read more");
    assert!(err.is_incomplete());
    // The whole frame decodes cleanly.
    let ok = Message::decode(&mut StreamBitReader::new(&whole[..]))?;
    assert_eq!(ok, say("hello world"));

    info!("all checks passed");
    Ok(())
}
