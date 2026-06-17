//! The `#[bin]` whole-message codec end to end — the flagship of the crate.
//!
//! Two real shapes:
//!   1. a 12-byte **DNS header** (RFC 1035 §4.1.1): a `#[bitfield]` flags word with
//!      nested `#[derive(BitEnum)]`s, the required-by-default builder, a `validate`
//!      soundness gate, and a byte-exact round-trip;
//!   2. a tiny **framed payload**: a `magic` constant, a length that is *derived*
//!      on write (`#[bw(calc)]`) and *consumed* on read (`#[br(temp)]`) rather than
//!      stored, and a `#[br(count = …)]` `Vec` driven by it.
//!
//! Run with: `cargo run -p bnb --example bin_message`

use bnb::{BitEnum, bin, bitfield, u3, u4};

// --- 1. The DNS header --------------------------------------------------------

/// The 4-bit DNS opcode; the catch-all keeps unknown values (dual-use).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum OpCode {
    Query,
    IQuery,
    Status,
    #[catch_all]
    Other(u4),
}

/// The 4-bit DNS response code.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum RCode {
    NoError,
    FormErr,
    ServFail,
    NxDomain,
    #[catch_all]
    Other(u4),
}

/// The 16-bit flags word: MSB-first packing (the RFC diagram order), big-endian.
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Flags {
    qr: bool,       // query (0) / response (1)
    opcode: OpCode, // 4 bits
    aa: bool,       // authoritative answer
    tc: bool,       // truncated
    rd: bool,       // recursion desired
    ra: bool,       // recursion available
    z: u3,          // reserved, must be zero
    rcode: RCode,   // 4 bits
}

/// The DNS message header. `#[bin(big)]` makes the whole 12-byte, byte-aligned
/// message a first-class `#[bin]` use; `validate` gates the builder without making
/// the parser reject anything (dual-use).
#[bin(big, validate = header_soundness)]
#[derive(Debug, Clone, PartialEq)]
struct Header {
    id: u16,
    flags: Flags,
    qdcount: u16,
    ancount: u16,
    nscount: u16,
    arcount: u16,
}

/// Construction-side soundness: the reserved `z` field must be zero (RFC 1035).
/// The *parser* never runs this — only `build()` does — so deliberately malformed
/// headers are still decodable for fuzzing/interop.
fn header_soundness(h: &Header) -> Result<(), String> {
    if h.flags.z() != u3::new(0) {
        return Err(format!("reserved z bits must be 0, got {}", h.flags.z()));
    }
    Ok(())
}

// --- 2. A magic-framed, length-prefixed payload -------------------------------

/// `magic(0xCAFE) | len(u8) | len bytes`. `len` is never stored: it is read into a
/// temp local that drives the `Vec`, and recomputed from `payload.len()` on write —
/// so the length can never drift from the data.
#[bin(big, magic = 0xCAFEu16)]
#[derive(Debug, Clone, PartialEq)]
struct Frame {
    #[br(temp)]
    #[bw(calc = self.payload.len() as u8)]
    len: u8,
    #[br(count = len)]
    payload: Vec<u8>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build a response header with the required-by-default builder. Forget a field
    // and `build()` tells you which one — the infix `with_*` setters can't.
    let flags = Flags::new()
        .with_qr(true)
        .with_opcode(OpCode::Query)
        .with_rd(true)
        .with_ra(true)
        .with_rcode(RCode::NoError);
    let header = Header::builder()
        .id(0x1234)
        .flags(flags)
        .qdcount(1)
        .ancount(1)
        .nscount(0)
        .arcount(0)
        .build()?;

    // Encode → exactly 12 bytes, byte-for-byte what the RFC layout dictates.
    let bytes = header.to_bytes()?;
    assert_eq!(bytes.len(), 12);
    assert_eq!(&bytes[..4], &[0x12, 0x34, 0x81, 0x80]); // id, then flags 0x8180
    println!("DNS header ({} bytes): {bytes:02x?}", bytes.len());

    // Decode is the exact inverse.
    let parsed = Header::decode_exact(&bytes)?;
    assert_eq!(parsed, header);
    println!(
        "  round-trips: opcode={:?}, rcode={:?}",
        parsed.flags.opcode(),
        parsed.flags.rcode()
    );

    // `validate` gates the builder — but the parser stays permissive. A header with
    // reserved bits set is still decodable (dual-use), yet can't be *built*.
    let malformed = Header::builder()
        .id(0)
        .flags(Flags::new().with_z(u3::new(0b101)))
        .qdcount(0)
        .ancount(0)
        .nscount(0)
        .arcount(0)
        .build();
    assert!(malformed.is_err());
    println!(
        "  builder rejects reserved-bits-set: {}",
        malformed.unwrap_err()
    );
    assert!(Header::decode_exact(&header_with_reserved_bits()).is_ok()); // parser does not

    // The framed payload: magic + derived length + bytes.
    let frame = Frame::builder()
        .payload(vec![0xDE, 0xAD, 0xBE, 0xEF])
        .build()?;
    let framed = frame.to_bytes()?;
    assert_eq!(framed, [0xCA, 0xFE, 0x04, 0xDE, 0xAD, 0xBE, 0xEF]); // magic, len=4, data
    println!("Frame ({} bytes): {framed:02x?}", framed.len());

    let back = Frame::decode_exact(&framed)?;
    assert_eq!(back, frame);
    assert_eq!(back.payload.len(), 4);

    // A bad magic is a clean, position-aware error — not a panic.
    let err = Frame::decode_exact(&[0x00, 0x00, 0x00]).unwrap_err();
    println!("  bad magic → {err}");

    println!("all round-trips verified ✓");
    Ok(())
}

/// A header whose reserved `z` bits are set — representable on the wire, so the
/// permissive parser accepts it even though `build()` would not.
fn header_with_reserved_bits() -> Vec<u8> {
    let mut bytes = vec![0u8; 12];
    bytes[3] = 0b0111_0000; // z bits inside the flags word
    bytes
}
