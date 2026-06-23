//! **streaming** — `StreamBitReader`: decode a *sequence* of messages off a forward-only `Read`
//! (a pipe, a log, a socket you read once), and use the `Incomplete` signal to stop cleanly at a
//! truncated tail. Forward-only — no seeking — unlike `BufSource` / `SeekReader`. (A different
//! angle from `framed`, which paired it with the `bytes` adapters.)
//!
//! Run with: `cargo run -p bitsandbytes --example streaming`

use bnb::{ErrorKind, StreamBitReader, bin};

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Event {
    code: u16,
    #[br(temp)]
    #[bw(calc = self.detail.len() as u8)]
    len: u8,
    #[br(count = len)]
    detail: Vec<u8>,
}

fn is_eof(kind: &ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::Incomplete { .. } | ErrorKind::UnexpectedEof { .. }
    )
}

fn main() {
    // Three events back-to-back, as if appended to a log.
    let mut wire = Vec::new();
    for ev in [
        Event {
            code: 1,
            detail: b"boot".to_vec(),
        },
        Event {
            code: 2,
            detail: b"warn".to_vec(),
        },
        Event {
            code: 3,
            detail: b"halt".to_vec(),
        },
    ] {
        wire.extend_from_slice(&ev.to_bytes().unwrap());
    }

    // Read them forward, one at a time, until the stream runs out.
    let mut r = StreamBitReader::new(wire.as_slice());
    let mut seen = 0;
    loop {
        match Event::decode_from(&mut r) {
            Ok(ev) => {
                println!("event: {ev:?}");
                seen += 1;
            }
            Err(e) if is_eof(&e.kind) => break, // clean end of stream
            Err(e) => panic!("decode error: {e}"),
        }
    }
    assert_eq!(seen, 3);

    // A truncated tail reports `Incomplete` (read-more / EOF), not a panic.
    let truncated = &wire[..wire.len() - 2];
    let mut r = StreamBitReader::new(truncated);
    let _ = Event::decode_from(&mut r); // event 1
    let _ = Event::decode_from(&mut r); // event 2
    let err = Event::decode_from(&mut r).unwrap_err(); // event 3 is cut short
    println!("truncated tail -> {err}");
    assert!(is_eof(&err.kind));

    println!("all checks passed");
}
