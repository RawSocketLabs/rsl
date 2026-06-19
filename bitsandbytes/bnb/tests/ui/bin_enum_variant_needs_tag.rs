//! Every `#[bin]` enum variant needs a `#[bin(tag = <value>)]` (or one `#[catch_all]`).
use bnb::bin;

#[bin(big, tag = u8)]
enum E {
    #[bin(tag = 1)]
    A(u8),
    B(u8), // no tag, no #[catch_all]
}

fn main() {}
