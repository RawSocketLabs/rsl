//! **`derived_lengths`** — counts the encoder fills in, and the caller may still forge.
//!
//! A count on the wire is redundant with the collection it sizes, so a hand-kept count
//! is a bug waiting for an edit. `count_prefix` covers the count that sits right before
//! its collection (see `tlv`). [`WireLen`] covers the rest: a count stored somewhere else
//! in the same message, or in a header struct that sizes collections beside it (the DNS
//! shape). `WireLen::auto()` is filled from the collection at encode; `WireLen::set(n)` is
//! written as given, right or wrong, so the same type builds a deliberately malformed
//! message for a test or a probe. Decode always yields what was on the wire.
//!
//! Run with: `cargo run -p bitsandbytes --example derived_lengths`
//!
//! [`WireLen`]: bnb::WireLen

use bnb::{ErrorKind, WireLen, bin};

/// The count is not next to what it sizes: a flag byte sits between them.
#[bin(big)]
#[derive(Debug, PartialEq)]
struct Batch {
    #[bw(auto_len = count(items))]
    len: WireLen<u8>,
    flags: u8,
    #[br(count = len.to_count())]
    items: Vec<u16>,
}

/// Counts in a header, collections in the message that holds it.
#[bin(big)]
#[derive(Debug, Clone, PartialEq)]
struct Header {
    id: u16,
    questions: WireLen<u16>,
    answers: WireLen<u16>,
}

#[bin(big, auto_len(header.questions = count(questions), header.answers = count(answers)))]
#[derive(Debug, PartialEq)]
struct Message {
    header: Header,
    #[br(count = header.questions.to_count())]
    questions: Vec<u8>,
    #[br(count = header.answers.to_count())]
    answers: Vec<u16>,
}

#[allow(clippy::print_stdout)] // This CLI demo intentionally prints its observable results.
fn main() {
    // --- the same struct ---
    let batch = Batch {
        len: WireLen::auto(),
        flags: 0x80,
        items: vec![1, 2, 3],
    };
    let wire = batch.to_bytes().unwrap();
    assert_eq!(wire, [3, 0x80, 0, 1, 0, 2, 0, 3]);
    // Decode reports the count that was written, so a re-encode is byte-identical.
    let back = Batch::decode_exact(&wire).unwrap();
    assert_eq!(back.len, WireLen::set(3));
    assert_eq!(back.to_bytes().unwrap(), wire);
    println!("same struct : {wire:02X?}");

    // --- a header that sizes its siblings ---
    let message = Message {
        header: Header {
            id: 0xBEEF,
            questions: WireLen::auto(),
            answers: WireLen::auto(),
        },
        questions: vec![1, 2, 3],
        answers: vec![0xAABB],
    };
    let wire = message.to_bytes().unwrap();
    assert_eq!(wire, [0xBE, 0xEF, 0, 3, 0, 1, 1, 2, 3, 0xAA, 0xBB]);
    println!("header      : {wire:02X?}");

    // --- a forged count: claims five answers and carries one ---
    let forged = Message {
        header: Header {
            id: 0xBEEF,
            questions: WireLen::auto(),
            answers: WireLen::set(5),
        },
        questions: vec![],
        answers: vec![0xAABB],
    };
    let wire = forged.to_bytes().unwrap();
    assert_eq!(wire, [0xBE, 0xEF, 0, 0, 0, 5, 0xAA, 0xBB]);
    // What a careful peer does with it: the message ends before its fifth answer.
    let error = Message::decode_exact(&wire).unwrap_err();
    assert!(matches!(error.kind, ErrorKind::UnexpectedEof { .. }));
    println!("forged      : {wire:02X?} (a decoder rejects it)");

    // --- a collection too long for its count is an error, never a truncated count ---
    let long = Batch {
        len: WireLen::auto(),
        flags: 0,
        items: vec![0; 300],
    };
    let error = long.to_bytes().unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::CountOutOfRange {
            count: 300,
            bits: 8
        }
    );
    println!("overflow    : 300 items do not fit a one-byte count");

    println!("all checks passed");
}
