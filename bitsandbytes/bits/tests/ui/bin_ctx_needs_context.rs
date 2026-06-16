//! A `#[bin(ctx(...))]` type declares context it needs, so it has no plain
//! `decode`/`decode_exact` (and no `BitDecode` impl) — it must be decoded with
//! context via `decode_with`. Calling the plain entry point is a compile error.
use bits::bin;

#[bin(ctx(tag: u8))]
struct Value {
    a: bits::u4,
    b: bits::u4,
}

fn main() {
    let _ = Value::decode_exact(&[0u8]);
}
