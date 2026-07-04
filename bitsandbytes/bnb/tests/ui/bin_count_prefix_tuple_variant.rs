//! `count_prefix` injects a *named* length field; a tuple variant's fields are
//! positional, so there is nowhere to put it — use a struct-style variant.
use bnb::bin;

#[bin]
enum Message {
    #[bin(magic = 0x01u8)]
    Data(#[brw(count_prefix = u8)] Vec<u8>),
    #[bin(magic = 0x02u8)]
    Ping { seq: u8 },
}

fn main() {}
