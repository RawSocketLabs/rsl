// All fields are whole bytes, so the cursor never leaves byte boundaries and the
// bit-stream codec is the wrong tool. The derive must reject this at compile time
// and steer the author to `#[binrw]`/`#[wire]`.
use bits::BitDecode;

#[derive(BitDecode)]
struct AllByteAligned {
    a: u8,
    b: u16,
}

fn main() {}
