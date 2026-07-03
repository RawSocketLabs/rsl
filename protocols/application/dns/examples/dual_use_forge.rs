//! **dual_use_forge** — the escape hatch. Section counts are [`WireLen`]: left `auto()`
//! they derive from their sections on encode, but `set(n)` pins a value that deliberately
//! *disagrees* — a malformed frame for fuzzing / interop testing. The codec emits exactly
//! what you pin; it enforces no policy.
//!
//! Run with: `cargo run -p dns --example dual_use_forge`

use dns::{Header, Message, State, WireLen};

fn main() {
    // A header claiming FIVE answers while carrying none: `ancount` is pinned to the lie,
    // the other three counts auto-derive (to 0) from their empty sections.
    let header = Header {
        id: 0x0001,
        state: State::new().with_response(true),
        qdcount: WireLen::auto(),
        ancount: WireLen::set(5), // ← the lie
        nscount: WireLen::auto(),
        arcount: WireLen::auto(),
    };
    let forged = Message {
        header,
        questions: vec![],
        answers: vec![], // reality: empty
        authorities: vec![],
        additional: vec![],
    };

    let wire = forged.to_bytes().expect("encodes");
    println!("forged header claims ancount=5 with an empty answer section:");
    println!("{wire:02x?}");

    // The codec wrote the pinned lie verbatim — bytes 6..8 are the answer count = 5 —
    // while qd/ns/ar auto-derived to 0.
    assert_eq!(&wire[6..8], &[0x00, 0x05], "pinned ancount is written");
    assert_eq!(&wire[4..6], &[0x00, 0x00], "qdcount auto-derived to 0");

    // A count-trusting decode then hits EOF looking for the 5 promised answers — exactly
    // how a real peer reacts to the malformed frame.
    let err = Message::decode_exact(&wire).unwrap_err();
    println!("\na count-trusting decode of the forged frame errors cleanly: {err}");

    // `Message::assemble` resets the counts to `auto()`, so the same sections encode with an
    // honest, self-consistent header.
    let honest = Message::assemble(forged.header, vec![], vec![], vec![], vec![]);
    assert_eq!(honest.header.ancount, WireLen::auto());
    assert_eq!(&honest.to_bytes().unwrap()[6..8], &[0x00, 0x00]);
    println!("`Message::assemble` derives an honest ancount=0 ✓");
}
