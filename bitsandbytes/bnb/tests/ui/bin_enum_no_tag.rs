//! A `#[bin]` enum must declare how the variant is selected: `tag` or `tag_from`.
use bnb::bin;

#[bin(big)]
enum E {
    #[bin(tag = 1)]
    A(u8),
}

fn main() {}
