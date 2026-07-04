//! **streaming** — `StreamBitReader`: decode a *sequence* of messages off a forward-only `Read`
//! (a pipe, a log, a socket you read once). Forward-only — no seeking — unlike `BufSource` /
//! `SeekReader`. (A different angle from `framed`, which paired it with the `bytes` adapters.)
//!
//! The point of this example is the **two truncation signals** and why they differ:
//!
//! - [`ErrorKind::Incomplete`] — *retryable*. A streaming source can't know whether more bytes
//!   will ever arrive, so running out mid-message means "read more and retry". Test for it with
//!   the shipped [`BitError::is_incomplete`]. Only the **caller** knows when the input is truly
//!   exhausted — ending the loop is the caller's decision, not the error's.
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
    // `Incomplete` — "read more and retry" — because *it* can't know the input is done.
    // *We* know this buffer is the whole input, so it's the caller, explicitly, that
    // turns the retryable signal into end-of-input. Any non-`Incomplete` error is a
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
                // Over a live pipe/socket we'd wait for more bytes and retry here.
                // Our source has nothing left to feed, so: end of input.
                println!("input exhausted after {seen} events ({e})");
                break;
            }
            Err(e) => panic!("definitive decode error: {e}"),
        }
    }
    assert_eq!(seen, 3);

    // The same truncated bytes, two different verdicts — that's the distinction:
    let truncated = &wire[..wire.len() - 2];

    // 1) Through a *stream*, the cut-short tail is `Incomplete` — retryable; for all the
    //    reader knows, the rest of event 3 is still in flight.
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
