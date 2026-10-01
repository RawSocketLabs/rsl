//! **streaming** — `StreamBitReader`: decode a *sequence* of messages off a forward-only `Read`
//! (a pipe, a log, a socket you read once). Forward-only — no seeking — unlike `BufSource` /
//! `SeekReader`. (A different angle from `framed`, which paired it with the `bytes` adapters.)
//!
//! The point of this example is the **two truncation signals** and why they differ:
//!
//! - [`ErrorKind::Incomplete`] — the source ran out mid-message. Test for it with
//!   [`BitError::is_incomplete`]. A `StreamBitReader` has already consumed input, so do
//!   **not** retry the failed field/message on it. Use `BitBuf` for retained-input retries.
//!   Only the caller knows whether the input is truly exhausted.
//! - [`ErrorKind::UnexpectedEof`] — *definitive*. A finite input (a slice) ends where it ends;
//!   the same truncated bytes are a hard EOF, no retry possible.
//!
//! [`ErrorKind::Incomplete`]: bnb::ErrorKind::Incomplete
//! [`ErrorKind::UnexpectedEof`]: bnb::ErrorKind::UnexpectedEof
//! [`BitError::is_incomplete`]: bnb::BitError::is_incomplete
//!
//! Run with: `cargo run -p bitsandbytes --example streaming`

use bnb::{BitReader, ErrorKind, StreamBitReader, bin};

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Event {
    code: u16,
    #[brw(count_prefix = u8)] // derived, never stored, checked at encode
    #[try_str]
    detail: Vec<u8>,
}

#[allow(clippy::print_stdout)] // This CLI demo intentionally prints its observable results.
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

    // Read them forward, one at a time. When the stream runs dry the reader says
    // `Incomplete`, because it cannot know whether the input is finite. This fixture
    // contains exactly three complete messages; we check that count below. In general,
    // `Incomplete` alone cannot distinguish clean EOF from a truncated final message.
    // Any non-`Incomplete` error is a
    // definitive decode failure (malformed data), never end-of-input.
    let mut r = StreamBitReader::new(wire.as_slice());
    let mut seen = 0;
    loop {
        match Event::decode(&mut r) {
            Ok(ev) => {
                println!("event: {ev:?}");
                seen += 1;
            }
            Err(e) if e.is_incomplete() => {
                // This fixture is exhausted. A live fragmented source should instead
                // use BitBuf; this reader cannot roll back an interrupted message.
                println!("input exhausted after {seen} events ({e})");
                break;
            }
            Err(e) => panic!("definitive decode error: {e}"),
        }
    }
    assert_eq!(seen, 3);

    // The same truncated bytes, two different verdicts — that's the distinction:
    let truncated = &wire[..wire.len() - 2];

    // 1) Through a stream, the cut-short tail is Incomplete, but consumed bytes cannot
    //    be recovered from this forward-only reader for a retry.
    let mut r = StreamBitReader::new(truncated);
    let _ = Event::decode(&mut r); // event 1
    let _ = Event::decode(&mut r); // event 2
    let err = Event::decode(&mut r).unwrap_err(); // event 3 is cut short
    println!("stream truncated tail -> {err}");
    assert!(err.is_incomplete()); // the retry signal, via the shipped predicate

    // 2) Through a *slice*, the very same bytes are `UnexpectedEof` — definitive; a finite
    //    input can't grow, so there is nothing to retry.
    let mut r = BitReader::new(truncated);
    let _ = Event::decode(&mut r); // event 1
    let _ = Event::decode(&mut r); // event 2
    let err = Event::decode(&mut r).unwrap_err(); // event 3 is cut short — for good
    println!("slice truncated tail  -> {err}");
    assert!(!err.is_incomplete());
    assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));

    println!("all checks passed");
}
