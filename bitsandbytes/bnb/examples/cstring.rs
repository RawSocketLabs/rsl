//! **cstring** — the shipped **NUL-terminated** codec, [`bnb::codecs::cstring`]: read
//! bytes until `0x00`, write them + a terminator. Two forms: raw bytes (`Vec<u8>`,
//! permissive — pairs with `#[try_str]` for display) and UTF-8 (`String` — decode errors
//! on invalid UTF-8, which a `String` physically can't hold). Write is **checked**: an
//! embedded NUL can't round-trip through this wire form, so it's refused rather than
//! silently truncated by the next decoder.
//!
//! Run with: `cargo run -p bitsandbytes --example cstring`

use bnb::bin;

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Entry {
    id: u16,
    #[br(parse_with = bnb::codecs::cstring::parse)]
    #[bw(write_with = bnb::codecs::cstring::write)]
    #[try_str]
    name: Vec<u8>, // raw bytes: any non-NUL content, dual-use permissive
    #[br(parse_with = bnb::codecs::cstring::parse_utf8)]
    #[bw(write_with = bnb::codecs::cstring::write_utf8)]
    title: String, // UTF-8: decode validates (a String can't hold invalid bytes)
    flags: u8,
}

fn main() {
    let e = Entry {
        id: 42,
        name: b"alpha".to_vec(),
        title: "héllo".into(),
        flags: 0x01,
    };
    let bytes = e.to_bytes().unwrap();
    // id(2) | "alpha" 00 | "héllo" (UTF-8) 00 | flags(1)
    println!("encoded: {bytes:02x?}");
    assert_eq!(bytes[2..8], *b"alpha\0");
    assert_eq!(Entry::decode_exact(&bytes).unwrap(), e);

    // The raw form renders naturally via #[try_str].
    println!("decoded: {:?}", Entry::decode_exact(&bytes).unwrap());

    // Checked write: an embedded NUL would decode back truncated, so encoding refuses it.
    let bad = Entry {
        id: 1,
        name: b"al\0pha".to_vec(),
        title: "ok".into(),
        flags: 0,
    };
    let err = bad.to_bytes().unwrap_err();
    println!("embedded NUL -> {err}");
    assert_eq!(err.field, Some("name"));

    println!("all checks passed");
}
