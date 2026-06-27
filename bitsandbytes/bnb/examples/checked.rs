//! **checked** — `try_map`: a fallible wire→value conversion that **rejects** an invalid byte at
//! decode time with `ErrorKind::Convert`. This is the deliberate-rejection counterpart to the
//! permissive defaults: use `try_map` for values that are genuinely *unrepresentable*, not
//! merely *unknown* (for unknown-but-valid, keep `#[catch_all]` / flag retention). It also
//! differs from construction-side `validate`, which gates `build()` but leaves the parser open.
//!
//! Run with: `cargo run -p bitsandbytes --example checked`

use bnb::{ErrorKind, bin};

/// A protocol version this build understands — only 1 and 2 exist.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Version {
    V1,
    V2,
}

/// The fallible conversion: an unknown version byte is a hard error, not a retained unknown.
fn to_version(raw: u8) -> Result<Version, String> {
    match raw {
        1 => Ok(Version::V1),
        2 => Ok(Version::V2),
        other => Err(format!("unsupported version {other}")),
    }
}

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Header {
    #[br(try_map = to_version)]
    #[bw(map = |v: &Version| match v { Version::V1 => 1u8, Version::V2 => 2u8 })]
    version: Version,

    #[br(temp)]
    #[bw(calc = self.name.len() as u8)]
    name_len: u8,

    #[try_str]
    #[br(count = name_len)]
    name: Vec<u8>,
}

fn main() {
    // A valid header round-trips.
    let h = Header {
        version: Version::V2,
        name: b"alpha".to_vec(),
    };
    let bytes = h.to_bytes().unwrap();
    println!("encoded: {bytes:02x?}");
    assert_eq!(Header::decode_exact(&bytes).unwrap(), h);
    println!("{h:#?}");

    // A wire version of 9 isn't representable — `try_map` rejects it at decode time.
    let bad = [0x09, 0x00];
    let err = Header::decode_exact(&bad).unwrap_err();
    println!("decoding version=9 -> {err}");
    assert!(matches!(err.kind, ErrorKind::Convert { .. }));
    assert_eq!(err.field, Some("version"));

    println!("all checks passed");
}
