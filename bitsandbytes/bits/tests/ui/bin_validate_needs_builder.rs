//! `validate` runs in `build()`, so it needs the builder — it is incompatible with
//! `read_only` (and `no_builder`).
use bits::bin;

#[bin(read_only, validate = check)]
struct Frame {
    a: bits::u4,
    b: bits::u4,
}

fn check(_: &Frame) -> Result<(), &'static str> {
    Ok(())
}

fn main() {}
