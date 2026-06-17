//! A `#[bin]` message that uses `#[br(restore_position)]` seeks, so its generated
//! `decode_from` is bound on `SeekSource`. Decoding through a forward-only
//! `StreamBitReader` (which is `Source` but not `SeekSource`) must be a compile
//! error — not a runtime `NotSeekable` surprise.

use bnb::{bin, StreamBitReader};

#[bin(big)]
#[derive(Debug)]
struct Peeked {
    #[br(restore_position)]
    tag: u8,
    body: u8,
}

fn main() {
    let data: &[u8] = &[0x01, 0x02];
    let mut r = StreamBitReader::new(data);
    let _ = Peeked::decode_from(&mut r);
}
