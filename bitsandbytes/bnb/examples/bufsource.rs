//! **bufsource** — `BufSource`: a bounded retain-and-seek buffer over a forward-only `Read` —
//! the "socket that *also* needs to seek" case. A message with a backward `restore_position`
//! decodes over it even though the underlying reader **can't** seek, because `BufSource` retains
//! the recent bytes. (Contrast `tcp`, which used `BufSource` purely forward, and `SeekReader`,
//! which needs a real `Read + Seek`. A `StreamBitReader` here would be a *compile error* — its
//! source isn't seekable.)
//!
//! Run with: `cargo run -p bitsandbytes --example bufsource`

use bnb::{BufSource, bin};
use std::io::Read;

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    flags: u8,
    #[br(restore_position)] // peek the next byte, then rewind so `value` re-reads it
    peek: u8,
    value: u16,
}

/// A forward-only `Read` (no `Seek`) that hands out one byte per call — like a socket.
struct Trickle<'a> {
    data: &'a [u8],
}
impl Read for Trickle<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.data.is_empty() || buf.is_empty() {
            return Ok(0);
        }
        buf[0] = self.data[0];
        self.data = &self.data[1..];
        Ok(1)
    }
}

fn main() -> Result<(), bnb::BitError> {
    let wire = [0x5A, 0xBC, 0xDE]; // flags=0x5A, value=0xBCDE; the peek sees value's high byte

    // The reader can't seek — but `BufSource` retains bytes, so the `restore_position` rewind
    // works anyway. (`Frame::decode` is bound on `SeekSource`; `BufSource` is one.)
    let mut src = BufSource::new(Trickle { data: &wire });
    let f = Frame::decode(&mut src)?;
    println!("{f:#?}");

    assert_eq!(f.flags, 0x5A);
    assert_eq!(f.peek, 0xBC); // peeked value's high byte, then rewound
    assert_eq!(f.value, 0xBCDE);

    println!("all checks passed");
    Ok(())
}
