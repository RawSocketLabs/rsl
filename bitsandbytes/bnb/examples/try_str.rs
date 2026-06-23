//! **try_str** — the `#[try_str]` field hint: a byte-buffer field renders in `Debug` as a
//! **string** when it's valid UTF-8, else as **hex bytes** — all-or-nothing, never lossy.
//!
//! It's *rendering only*: the field stays a `Vec<u8>` storing the raw bytes (sized by `count`,
//! like any buffer), so the parser stays permissive — a non-UTF-8 value still decodes fine, it
//! just prints as bytes. That faithfulness is the point: the view never misrepresents the wire.
//! (`Debug` is what `tracing`'s `?` and `{:#?}` use — so this is what cleans up log output.)
//!
//! Run with: `cargo run -p bitsandbytes --example try_str`

use bnb::bin;

/// A label record: an id and a length-prefixed name. `#[try_str]` makes `name` print as text.
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Record {
    id: u16,
    #[br(temp)]
    #[bw(calc = self.name.len() as u8)]
    len: u8,
    #[br(count = len)]
    #[try_str]
    name: Vec<u8>,
}

fn main() {
    // A text value renders as a quoted string.
    let text = Record {
        id: 1,
        name: b"sensor-7".to_vec(),
    };
    println!("{text:#?}");
    assert_eq!(
        Record::decode_exact(&text.to_bytes().unwrap()).unwrap(),
        text
    );

    // A binary value (not valid UTF-8) falls back to hex bytes — same field, no panic, no `�`.
    let binary = Record {
        id: 2,
        name: vec![0x00, 0xFF, 0xC0, 0xDE],
    };
    println!("{binary:#?}");
    assert_eq!(
        Record::decode_exact(&binary.to_bytes().unwrap()).unwrap(),
        binary
    );

    println!("all checks passed");
}
