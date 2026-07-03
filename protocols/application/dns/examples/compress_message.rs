//! **compress_message** — encode a response two ways and compare. `to_compressed_bytes`
//! (RFC 1035 §4.1.4) points a repeated name back to its first occurrence, so the wire
//! form shrinks while decoding to the same message.
//!
//! Run with: `cargo run -p dns --example compress_message`

use dns::{Header, Message, QClass, QType, Question, RClass, RData, RType, Record, State, WireLen};
use std::net::Ipv4Addr;

fn main() {
    // A response where the question and answer share the name `www.example.com`.
    let name = "www.example.com";
    let question = Question {
        name: name.parse().unwrap(),
        qtype: QType::A,
        qclass: QClass::Internet,
    };
    let answer = Record {
        name: name.parse().unwrap(),
        rtype: RType::A,
        class: RClass::Internet,
        ttl: 60,
        rdlength: WireLen::auto(),
        data: RData::A(Ipv4Addr::new(1, 2, 3, 4)),
    };
    let header = Header {
        id: 0x1234,
        state: State::new().with_response(true),
        qdcount: WireLen::auto(),
        ancount: WireLen::auto(),
        nscount: WireLen::auto(),
        arcount: WireLen::auto(),
    };
    let msg = Message::assemble(header, vec![question], vec![answer], vec![], vec![]);

    let plain = msg.to_bytes().unwrap();
    let compressed = msg.to_compressed_bytes().unwrap();

    println!("uncompressed: {} bytes", plain.len());
    println!("compressed:   {} bytes", compressed.len());
    println!("compressed:   {compressed:02x?}");

    // The answer's name is now the 2-byte pointer `c0 0c` (→ the question at offset 12)
    // instead of a repeated 17-byte `www.example.com`.
    let saved = plain.len() - compressed.len();
    println!("\nsaved {saved} bytes by pointing the answer name at the question");

    // Both forms decode to the same message — decode follows the pointer inline. (A
    // decoded message carries `Set` counts while `msg` built them `Auto`, so compare the
    // round-tripped bytes.)
    assert_eq!(
        Message::decode_exact(&compressed)
            .unwrap()
            .to_bytes()
            .unwrap(),
        plain
    );
    assert_eq!(
        Message::decode_exact(&plain).unwrap().to_bytes().unwrap(),
        plain
    );
    println!("both forms round-trip to the same Message ✓");
}
