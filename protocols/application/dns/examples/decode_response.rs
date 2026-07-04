//! **decode_response** — decode a real DNS response packet and walk its sections,
//! showing that unknown record types are preserved as raw `Custom` bytes (dual-use),
//! never misparsed.
//!
//! Run with: `cargo run -p dns --example decode_response`

use dns::{Message, RData};

fn main() {
    // A response carrying: a `www.example.com` A record, an SOA in the authority section,
    // and an (unregistered) TYPE=9999 record in the additional section.
    let wire: &[u8] = &[
        0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, // header
        // question: www.example.com A IN
        0x03, b'w', b'w', b'w', 0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o',
        b'm', 0x00, 0x00, 0x01, 0x00, 0x01, //
        // answer: www.example.com (pointer) A IN ttl=60 1.2.3.4
        0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04, 0x01, 0x02, 0x03,
        0x04, //
        // authority: example.com (pointer to 0x10) SOA ...
        0xc0, 0x10, 0x00, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00,
        0x22, // hdr, rdlen=34
        0x03, b'n', b's', b'1', 0xc0, 0x10, // MNAME ns1.example.com
        0x05, b'a', b'd', b'm', b'i', b'n', 0xc0, 0x10, // RNAME admin.example.com
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x0e, 0x10, 0x00, 0x00, 0x02, 0x58, 0x00, 0x24, 0xea,
        0x00, 0x00, 0x00, 0x01, 0x2c, // serial/refresh/retry/expire/minimum
        // additional: root name, TYPE=9999 (unknown), IN, ttl=0, rdlen=3, raw bytes
        0x00, 0x27, 0x0f, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xde, 0xad, 0xbe,
    ];

    let msg = Message::decode_exact(wire).expect("decodes");

    println!(
        "id=0x{:04x}  response={}  rcode={:?}",
        msg.header.id,
        msg.header.is_response(),
        msg.header.rcode()
    );
    for q in &msg.questions {
        println!("QUESTION  {}  {:?} {:?}", q.name, q.qtype, q.qclass);
    }
    for (section, records) in [
        ("ANSWER", &msg.answers),
        ("AUTHORITY", &msg.authorities),
        ("ADDITIONAL", &msg.additional),
    ] {
        for r in records {
            let rendered = match &r.data {
                RData::A(ip) => format!("A     {ip}"),
                RData::Aaaa(ip) => format!("AAAA  {ip}"),
                RData::Cname(n) => format!("CNAME {n}"),
                RData::Ns(n) => format!("NS    {n}"),
                RData::Soa(soa) => format!("SOA   {} {}", soa.mname, soa.rname),
                RData::Custom { rtype, bytes } => {
                    // The dual-use fix: an unknown type keeps its raw RDATA, not a misparse.
                    format!("{rtype:?}  <{} raw bytes> {bytes:02x?}", bytes.len())
                }
                other => format!("{other:?}"),
            };
            println!("{section:<10} {:<20} {rendered}", r.name.to_string());
        }
    }

    // The unknown TYPE=9999 record survived as raw bytes.
    assert!(matches!(
        &msg.additional[0].data,
        RData::Custom { bytes, .. } if bytes == &[0xde, 0xad, 0xbe]
    ));
    println!("\nunknown record type preserved verbatim ✓");
}
