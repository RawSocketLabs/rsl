//! `validate` (ROADMAP Phase 2, P2.8 — the dual-use answer to binrw `pre_assert`):
//! a construction-soundness check run by `build()`. It is a free function
//! `fn(&Self) -> Result<(), impl Display>` (not a method, so it isn't mistaken for
//! protocol-context validity). A failure is `BuilderError::Invalid`. Crucially the
//! **parser stays permissive** — `decode` never runs it, so a non-conformant value
//! on the wire still parses (the dual-use rule).

use bnb::{BuilderError, bin, u4};

#[bin(validate = check_header)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Header {
    version: u4,
    flags: u4,
    length: u8,
}

fn check_header(h: &Header) -> Result<(), String> {
    if h.length == 0 {
        Err("length must be non-zero".to_string())
    } else {
        Ok(())
    }
}

#[test]
fn validate_passes_for_a_sound_value() {
    let h = Header::builder()
        .version(u4::new(1))
        .flags(u4::new(0))
        .length(8)
        .build()
        .unwrap();
    assert_eq!(Header::decode_exact(&h.to_bytes().unwrap()).unwrap(), h);
}

#[test]
fn validate_rejects_an_unsound_value_at_build() {
    let err = Header::builder()
        .version(u4::new(1))
        .flags(u4::new(0))
        .length(0)
        .build()
        .unwrap_err();
    assert!(matches!(err, BuilderError::Invalid(_)));
}

#[test]
fn parser_stays_permissive() {
    // A length-0 header (which `build()` would reject) is constructed via a struct
    // literal and decodes fine — the parser never validates.
    let unsound = Header {
        version: u4::new(1),
        flags: u4::new(0),
        length: 0,
    };
    let bytes = unsound.to_bytes().unwrap();
    assert_eq!(Header::decode_exact(&bytes).unwrap(), unsound);
}
