//! **peek** — `restore_position` over a `SeekReader`: read a field, then rewind so a following
//! field re-reads the same bytes. The "look at a discriminant before committing to a layout"
//! move — which needs a seekable source (a backward seek), unlike `archive`'s forward
//! random-access. (A second `SeekReader` orchestration, and the read/rewind idiom `dns` uses
//! for compression pointers, distilled.)
//!
//! Run with: `cargo run -p bitsandbytes --example peek`

use bnb::{SeekReader, bin};
use std::io::Cursor;

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    // Peek the leading type byte *without consuming it* (the cursor rewinds after) — useful to
    // branch on before parsing — then read the real header, which re-reads that same byte.
    #[br(restore_position)]
    peeked_type: u8,
    msg_type: u8,
    length: u16,
}

fn main() -> Result<(), bnb::BitError> {
    let wire = vec![0x07, 0x12, 0x34]; // type = 7, length = 0x1234
    let mut src = SeekReader::new(Cursor::new(wire));

    let frame = Frame::decode(&mut src)?;
    println!("{frame:#?}");

    // `restore_position` rewound the cursor, so both fields saw the same first byte.
    assert_eq!(frame.peeked_type, frame.msg_type);
    assert_eq!(frame.msg_type, 0x07);
    assert_eq!(frame.length, 0x1234);

    println!("all checks passed");
    Ok(())
}
