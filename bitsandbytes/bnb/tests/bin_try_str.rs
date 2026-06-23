//! `#[try_str]`: a `Debug`-rendering hint — render a byte-buffer field as a string when it is
//! valid UTF-8, else as hex bytes (all-or-nothing, never lossy). Rendering only; the codec is
//! unaffected.

use bnb::{bin, u4};

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Msg {
    id: u8,
    #[br(temp)]
    #[bw(calc = self.name.len() as u8)]
    len: u8,
    #[br(count = len)]
    #[try_str]
    name: Vec<u8>,
}

#[test]
fn valid_utf8_renders_as_string() {
    let m = Msg {
        id: 1,
        name: b"hello".to_vec(),
    };
    let dbg = format!("{m:?}");
    assert!(dbg.contains(r#"name: "hello""#), "got: {dbg}");
    // The codec is unaffected — it still round-trips the raw bytes.
    assert_eq!(Msg::decode_exact(&m.to_bytes().unwrap()).unwrap(), m);
}

#[test]
fn invalid_utf8_falls_back_to_hex_bytes() {
    let m = Msg {
        id: 1,
        name: vec![0xDE, 0xAD, 0xBE, 0xEF],
    };
    let dbg = format!("{m:?}");
    assert!(dbg.contains("name: [de, ad, be, ef]"), "got: {dbg}");
    assert_eq!(Msg::decode_exact(&m.to_bytes().unwrap()).unwrap(), m);
}

// `#[try_str]` also works inside a canonical (`#[reserved]`-bearing) message, whose `Debug` is
// already custom (it excludes the hidden `encode_mode`).
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    version: u4,
    #[reserved]
    rsv: u4,
    #[br(temp)]
    #[bw(calc = self.label.len() as u8)]
    len: u8,
    #[br(count = len)]
    #[try_str]
    label: Vec<u8>,
}

#[test]
fn try_str_in_a_canonical_struct() {
    let f = Frame::builder()
        .version(u4::new(1))
        .label(b"hi".to_vec())
        .build()
        .unwrap();
    let dbg = format!("{f:?}");
    assert!(dbg.contains(r#"label: "hi""#), "got: {dbg}");
    assert!(!dbg.contains("encode_mode"), "mode must stay hidden: {dbg}");
}
