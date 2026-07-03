//! **dual_use_forge** — the escape hatch. The codec emits exactly what you construct,
//! enforcing no policy: here we forge a header whose `ancount` deliberately disagrees
//! with the actual answer section (a malformed frame for fuzzing / interop testing). The
//! guided `Message::assemble` would keep them in sync; setting the fields directly does
//! not.
//!
//! Run with: `cargo run -p dns --example dual_use_forge`

use dns::{Header, Message, State};

fn main() {
    // A well-formed header, then a message with ZERO answers but a header claiming five.
    let header = Header {
        id: 0x0001,
        state: State::new().with_response(true),
        qdcount: 0,
        ancount: 5, // ← a lie: there are no answers
        nscount: 0,
        arcount: 0,
    };
    // Construct the fields directly (not `assemble`, which would recompute the counts).
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

    // The codec wrote the lie verbatim — bytes 6..8 are the answer count = 5.
    assert_eq!(&wire[6..8], &[0x00, 0x05]);

    // A permissive decoder that trusts the count then hits EOF looking for answers —
    // exactly the behavior a real peer would exhibit (the frame is malformed on purpose).
    let err = Message::decode_exact(&wire).unwrap_err();
    println!("\na count-trusting decode of the forged frame errors cleanly: {err}");

    // The guided constructor, by contrast, keeps the wire self-consistent.
    let honest = Message::assemble(forged.header, vec![], vec![], vec![], vec![]);
    assert_eq!(honest.header.ancount, 0);
    println!("`Message::assemble` derives an honest ancount=0 ✓");
}
