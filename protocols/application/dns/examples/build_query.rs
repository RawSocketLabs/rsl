//! **build_query** — construct a recursive A-record query, encode it to wire bytes, and
//! confirm it round-trips. The encode/construction path.
//!
//! Run with: `cargo run -p dns --example build_query`

use dns::{Message, QClass, QType, Question};

fn main() {
    let question = Question {
        name: "www.example.com".parse().expect("valid name"),
        qtype: QType::A,
        qclass: QClass::Internet,
    };
    let msg = Message::query(0x1234, question);

    let wire = msg.to_bytes().expect("encodes");
    println!("query for www.example.com A IN ({} bytes):", wire.len());
    println!("{wire:02x?}");

    // Header: id=0x1234, RD set (0x0100), qdcount=1.
    assert_eq!(&wire[..4], &[0x12, 0x34, 0x01, 0x00]);
    assert_eq!(&wire[4..6], &[0x00, 0x01]); // qdcount = 1

    // Round-trips. (A decoded message carries `Set` counts while `msg` built them `Auto`,
    // so compare the round-tripped bytes — bnb's real round-trip contract.)
    assert_eq!(
        Message::decode_exact(&wire).unwrap().to_bytes().unwrap(),
        wire
    );
    println!("\nround-trips ✓");
}
