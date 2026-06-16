//! `#[br(if(...))]` makes a field conditional, so it must be an `Option<_>`.
use bits::bin;

#[bin]
struct Frame {
    flag: bits::u4,
    #[br(if(flag != bits::u4::new(0)))]
    value: u16,
}

fn main() {}
