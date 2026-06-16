//! `#[wire]` — a DNS-style header in one attribute.
//!
//! Without `#[wire]`, a header like this needs the hand-written triad: a
//! `#[binrw]` struct, a `#[derive(Builder)]`, a *private* collapsed `state`
//! field with `#[bw(calc = …)]` to reassemble it on write, three public fields
//! each `#[bw(ignore)] #[br(calc = state.x())]` to disassemble it on read, and
//! `#[bw(map = …)] + #[bw(import(…))]` to auto-count the sections. `#[wire]`
//! folds all of that — and leaves the full binrw attribute surface available as
//! an escape hatch (`#[br(count = …)]` below).
//!
//! Run: `cargo run -p bits --example wire_header`

use binrw::{BinRead, BinWrite};
use bnb::{BitEnum, bitflags, u4, wire};
use std::io::Cursor;

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum OpCode {
    Query,
    InverseQuery,
    Status,
    #[catch_all]
    Reserved(u4),
}

#[bitflags(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Flags {
    response: bool,
    authoritative: bool,
    truncated: bool,
    recursion_desired: bool,
    recursion_available: bool,
}

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

/// A response must not be marked truncated with zero answers (illustrative).
fn soundness(h: &Header) -> Result<(), String> {
    if h.flags.truncated() && h.answers.is_empty() {
        return Err("truncated response with no answers".into());
    }
    Ok(())
}

#[wire(big, group(opcode, flags, rcode => u16), validate = soundness)]
#[derive(Debug, Clone, PartialEq)]
struct Header {
    /// Transaction id.
    id: u16,

    // These three pack into one 16-bit word on the wire (a private #[bitfield]),
    // but stay first-class in the builder and as fields.
    opcode: OpCode,
    flags: Flags,
    rcode: RCode,

    /// Question count — derived from the section on write, never stored.
    #[update(self.questions.len() as u16)]
    qdcount: u16,
    /// Answer count — derived on write.
    #[update(self.answers.len() as u16)]
    ancount: u16,

    /// The questions (escape hatch: a raw binrw `count`).
    #[br(count = qdcount)]
    #[builder(default)]
    questions: Vec<u16>,
    /// The answers.
    #[br(count = ancount)]
    #[builder(default)]
    answers: Vec<u16>,
}

fn main() {
    // Build a compliant header. The builder calls out any unset required field;
    // counts are filled in automatically from the sections.
    let header = Header::builder()
        .id(0x1234)
        .opcode(OpCode::Query)
        .flags(Flags::RECURSION_DESIRED)
        .rcode(RCode::NoError)
        .questions(vec![0x000C, 0x0001])
        .build()
        .unwrap();

    let mut buf = Cursor::new(Vec::new());
    header.write(&mut buf).unwrap();
    let bytes = buf.into_inner();
    println!("encoded {} bytes: {:02x?}", bytes.len(), bytes);

    let decoded = Header::read(&mut Cursor::new(&bytes)).unwrap();
    println!("decoded: {decoded:?}");
    assert_eq!(decoded, header);
    println!(
        "round-trip ok; qdcount auto-counted = {}",
        decoded.questions.len()
    );

    // Soundness gates construction (the compliant default)...
    let bad = Header::builder()
        .id(1)
        .opcode(OpCode::Query)
        .flags(Flags::TRUNCATED)
        .rcode(RCode::NoError)
        .build();
    println!("invalid build rejected: {}", bad.unwrap_err());

    // ...but the dual-use escape hatch still lets you emit malformed traffic,
    // and the parser itself never rejects representable input.
    let malformed = Header::builder()
        .id(1)
        .opcode(OpCode::Query)
        .flags(Flags::TRUNCATED)
        .rcode(RCode::NoError)
        .check_soundness(false)
        .build()
        .unwrap();
    println!(
        "malformed message built via escape hatch: truncated={}",
        malformed.flags.truncated()
    );
}
