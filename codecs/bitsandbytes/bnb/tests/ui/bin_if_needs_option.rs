//! `#[br(if(...))]` makes a field conditional, so it must be an `Option<_>`.
use bnb::bin;

#[bin]
struct Frame {
    flag: bnb::u4,
    #[br(if(flag != bnb::u4::new(0)))]
    value: u16,
}

fn main() {}
