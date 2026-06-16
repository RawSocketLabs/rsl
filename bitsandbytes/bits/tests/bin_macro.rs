//! `#[bin]` foundation (ROADMAP Phase 2, P2.0): one attribute macro folds the
//! codec and the required-by-default builder, lowering to
//! `#[derive(BitDecode, BitEncode, BitsBuilder)]` + `#[bit_stream(...)]`. Field
//! directives (`#[br]`/`#[bw]`) arrive in later chunks.

use bits::{BuilderError, bin, u4, u12};

#[bin]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Frame {
    version: u4,
    #[builder(default)]
    flags: u4,
    payload_len: u12,
}

#[test]
fn folds_codec_and_builder() {
    let f = Frame::builder()
        .version(u4::new(4))
        .payload_len(u12::new(100))
        .build()
        .unwrap();
    assert_eq!(f.flags, u4::new(0), "builder default");

    let bytes = f.to_bytes().unwrap();
    assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
}

#[test]
fn enforces_required_fields() {
    let err = Frame::builder().version(u4::new(4)).build().unwrap_err();
    assert_eq!(err, BuilderError::MissingField("payload_len"));
}

#[bin(bit_order = lsb)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct LsbWord {
    a: u4,
    b: u12,
}

#[test]
fn bit_order_option_lowers() {
    let v = LsbWord {
        a: u4::new(0xA),
        b: u12::new(0),
    };
    // LSB-first packs the first field into the low bits.
    assert_eq!(v.to_bytes().unwrap()[0] & 0x0F, 0xA);
}

#[bin(read_only)]
#[derive(Debug, PartialEq, Eq)]
struct ReadOnly {
    a: u4,
    b: u12,
}

#[test]
fn read_only_decodes_only() {
    let r = ReadOnly::peek(&[0xAB, 0xCD]).unwrap();
    assert_eq!(
        r,
        ReadOnly {
            a: u4::new(0xA),
            b: u12::new(0xBCD)
        }
    );
    // No `to_bytes`/builder is generated for a read_only type (enforced at compile
    // time — only Decode is derived).
}
