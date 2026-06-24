//! **versioned** — `#[bin]` `if` driven by a **version** field (a different driver than
//! `conditional`'s per-bit flags): a v2 message carries fields a v1 message doesn't, so old and
//! new peers share one wire type. The version is `try_map`-checked, so an unknown version is
//! rejected at decode.
//!
//! Run with: `cargo run -p bitsandbytes --example versioned`

use bnb::bin;

/// Reject a version this build doesn't speak (a `try_map` used purely as a parse-time guard).
fn check_version(raw: u8) -> Result<u8, String> {
    if (1..=2).contains(&raw) {
        Ok(raw)
    } else {
        Err(format!("unsupported version {raw}"))
    }
}

/// A length-prefixed label (v2 only).
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Label {
    #[br(temp)]
    #[bw(calc = self.text.len() as u8)]
    len: u8,
    #[br(count = len)]
    #[try_str]
    text: Vec<u8>,
}

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Event {
    #[br(try_map = check_version)]
    #[bw(map = |v: &u8| *v)]
    version: u8,
    id: u32,
    // v2 extended the event with a priority and a label; a v1 event stops after `id`.
    #[br(if(version >= 2))]
    priority: Option<u8>,
    #[br(if(version >= 2))]
    label: Option<Label>,
}

fn main() {
    // A v2 event carries the extra fields.
    let v2 = Event {
        version: 2,
        id: 1001,
        priority: Some(5),
        label: Some(Label {
            text: b"deploy".to_vec(),
        }),
    };
    let bytes = v2.to_bytes().unwrap();
    println!("v2 event: {:>2} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Event::decode_exact(&bytes).unwrap(), v2);
    println!("{v2:#?}");

    // A v1 event omits them entirely — they aren't on the wire.
    let v1 = Event {
        version: 1,
        id: 1002,
        priority: None,
        label: None,
    };
    let bytes = v1.to_bytes().unwrap();
    println!("v1 event: {:>2} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Event::decode_exact(&bytes).unwrap(), v1);

    println!("all checks passed");
}
