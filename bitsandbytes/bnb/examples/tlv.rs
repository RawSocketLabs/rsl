//! **tlv** — a Type-Length-Value-style codec: a document is a `count`-driven sequence of
//! heterogeneous, self-describing records. A leading type byte (each variant's `magic`)
//! dispatches the record; its body — fixed, or length-prefixed for the variable ones — follows.
//! The "build your own extensible wire format" orchestration: enum `magic` dispatch + `count` +
//! `temp`/`calc` lengths, composed into one message.
//!
//! Run with: `cargo run -p bitsandbytes --example tlv`

use bnb::bin;

/// One record. The leading byte (its `magic`) is the type tag; the body follows.
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
enum Field {
    #[bin(magic = 0x01u8)]
    Version(u16),
    #[bin(magic = 0x02u8)]
    Name {
        #[br(temp)]
        #[bw(calc = text.len() as u8)]
        len: u8,
        #[br(count = len)]
        text: Vec<u8>,
    },
    #[bin(magic = 0x03u8)]
    Ttl(u32),
    #[bin(magic = 0x04u8)]
    Compression(u8),
}

/// A document: a record count, then that many self-describing records.
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Document {
    #[br(temp)]
    #[bw(calc = self.fields.len() as u8)]
    count: u8,
    #[br(count = count)]
    #[nested]
    fields: Vec<Field>,
}

fn main() {
    let doc = Document {
        fields: vec![
            Field::Version(2),
            Field::Name {
                text: b"sensor-7".to_vec(),
            },
            Field::Ttl(3600),
            Field::Compression(1),
        ],
    };
    let bytes = doc.to_bytes().unwrap();
    println!("document: {} bytes  {bytes:02x?}", bytes.len());
    let back = Document::decode_exact(&bytes).unwrap();
    assert_eq!(back, doc);
    println!("{back:#?}");
    println!("all checks passed");
}
