//! `#[wire]` — folds binrw + builder + bit-groups + derived fields + soundness.
#![cfg(feature = "binrw")]

use binrw::{BinRead, BinWrite};
use bits::{bitflags, wire, u4, BitEnum};
use std::io::Cursor;

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum OpCode {
    Query,
    IQuery,
    Status,
    #[catch_all]
    Other(u4),
}

#[bitflags(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Flags {
    qr: bool,
    aa: bool,
    tc: bool,
    rd: bool,
    ra: bool,
}

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum RCode {
    NoError,
    FormErr,
    ServFail,
    #[catch_all]
    Other(u4),
}

#[wire(big, group(opcode, flags, rcode => u16))]
#[derive(Debug, Clone, PartialEq)]
struct Header {
    id: u16,
    opcode: OpCode,
    flags: Flags,
    rcode: RCode,
    #[update(self.records.len() as u16)]
    count: u16,
    #[br(count = count)]
    #[builder(default)]
    records: Vec<u16>,
}

#[test]
fn builder_roundtrip() {
    let h = Header::builder()
        .id(0x1234)
        .opcode(OpCode::Status)
        .flags(Flags::empty())
        .rcode(RCode::NoError)
        .records(vec![0xAAAA])
        .build()
        .unwrap();

    let mut buf = Cursor::new(Vec::new());
    h.write(&mut buf).unwrap();
    // id=0x1234 | group: opcode=2 (high nibble) flags=0 rcode=0 => 0x2000 | count=1 | 0xAAAA
    assert_eq!(buf.get_ref().as_slice(), &[0x12, 0x34, 0x20, 0x00, 0x00, 0x01, 0xAA, 0xAA]);

    let back = Header::read(&mut Cursor::new(buf.get_ref())).unwrap();
    assert_eq!(back, h);
    // The derived count is recomputed on write, never stored.
    assert_eq!(back.records, vec![0xAAAA]);
}

#[test]
fn builder_requires_unset_field() {
    let err = Header::builder().id(1).build().unwrap_err();
    assert_eq!(err.field(), Some("opcode"));
}

// ---- soundness validation (dual-use) ----

fn sound_check(s: &Sound) -> Result<(), String> {
    if s.id == 0 {
        Err("id must be non-zero".into())
    } else {
        Ok(())
    }
}

#[wire(big, validate = sound_check)]
#[derive(Debug, Clone, PartialEq)]
struct Sound {
    id: u16,
    value: u8,
}

#[test]
fn validate_gates_build_but_not_parse() {
    // check_soundness defaults true -> an invalid value is rejected at build.
    let err = Sound::builder().id(0).value(5).build().unwrap_err();
    assert!(matches!(err, bits::BuilderError::Invalid(_)));
    assert_eq!(err.to_string(), "soundness check failed: id must be non-zero");

    // A valid value builds.
    assert_eq!(Sound::builder().id(1).value(5).build().unwrap().id, 1);

    // Escape hatch: turn the check off to construct a deliberately bad message.
    let bad = Sound::builder()
        .id(0)
        .value(5)
        .check_soundness(false)
        .build()
        .unwrap();
    assert_eq!(bad.id, 0);

    // The parser stays permissive: a malformed message reads back fine...
    let mut buf = Cursor::new(Vec::new());
    bad.write(&mut buf).unwrap();
    assert_eq!(buf.get_ref().as_slice(), &[0x00, 0x00, 0x05]);
    let parsed = Sound::read(&mut Cursor::new(buf.get_ref())).unwrap();
    assert_eq!(parsed.id, 0);

    // ...but opt-in validate() (check_soundness is true on parse) catches it.
    assert!(parsed.validate().is_err());
}

// ---- builder-only fields (off the wire) ----

#[wire(big)]
#[derive(Debug, Clone, PartialEq)]
struct WithMeta {
    id: u16,
    #[builder_only(default = 0)]
    tag: u8,
}

#[test]
fn builder_only_is_off_wire() {
    let m = WithMeta::builder().id(0x0102).tag(7).build().unwrap();
    assert_eq!(m.tag, 7);

    let mut buf = Cursor::new(Vec::new());
    m.write(&mut buf).unwrap();
    assert_eq!(buf.get_ref().as_slice(), &[0x01, 0x02]); // tag not serialized

    let back = WithMeta::read(&mut Cursor::new(buf.get_ref())).unwrap();
    assert_eq!(back.tag, 0); // read default
    assert_eq!(back.id, 0x0102);
}

// ---- multiple groups + little-endian ----

#[wire(little, group(a, b => u8), group(c, d => u8))]
#[derive(Debug, Clone, PartialEq)]
struct TwoGroups {
    a: u4,
    b: u4,
    mid: u16,
    c: u4,
    d: u4,
}

#[test]
fn multiple_groups_little_endian() {
    let m = TwoGroups::builder()
        .a(u4::new(0x1))
        .b(u4::new(0x2))
        .mid(0x0304)
        .c(u4::new(0x5))
        .d(u4::new(0x6))
        .build()
        .unwrap();

    let mut buf = Cursor::new(Vec::new());
    m.write(&mut buf).unwrap();
    // group1 byte 0x12 | mid 0x0304 little-endian = 04 03 | group2 byte 0x56
    assert_eq!(buf.get_ref().as_slice(), &[0x12, 0x04, 0x03, 0x56]);

    assert_eq!(TwoGroups::read(&mut Cursor::new(buf.get_ref())).unwrap(), m);
}

// ---- no_builder: codec + groups only ----

#[wire(big, no_builder, group(hi, lo => u8))]
#[derive(Debug, Clone, PartialEq)]
struct NoBuild {
    hi: u4,
    lo: u4,
    rest: u16,
}

#[test]
fn no_builder_codec_only() {
    let m = NoBuild {
        hi: u4::new(0xA),
        lo: u4::new(0xB),
        rest: 0xCAFE,
    };
    let mut buf = Cursor::new(Vec::new());
    m.write(&mut buf).unwrap();
    assert_eq!(buf.get_ref().as_slice(), &[0xAB, 0xCA, 0xFE]);
    assert_eq!(NoBuild::read(&mut Cursor::new(buf.get_ref())).unwrap(), m);
}

// ---- escape hatch: raw binrw map passes through untouched ----

#[wire(big)]
#[derive(Debug, Clone, PartialEq)]
struct WithMap {
    raw: u8,
    #[br(map = |x: u8| x as u16 + 1)]
    #[bw(map = |x: &u16| (*x - 1) as u8)]
    #[builder(default)]
    adjusted: u16,
}

#[test]
fn binrw_map_escape_hatch() {
    let m = WithMap::builder().raw(9).adjusted(42).build().unwrap();
    let mut buf = Cursor::new(Vec::new());
    m.write(&mut buf).unwrap();
    // adjusted 42 -> wire (42 - 1) = 41 in one byte
    assert_eq!(buf.get_ref().as_slice(), &[9, 41]);
    let back = WithMap::read(&mut Cursor::new(buf.get_ref())).unwrap();
    assert_eq!(back.adjusted, 42); // wire 41 -> 41 + 1
}

// ---- escape hatch: binrw magic passes through ----

#[wire(big)]
#[derive(Debug, Clone, PartialEq)]
struct WithMagic {
    #[brw(magic = 0x1234u16)]
    body: u16,
}

#[test]
fn binrw_magic_escape_hatch() {
    let m = WithMagic::builder().body(0xBEEF).build().unwrap();
    let mut buf = Cursor::new(Vec::new());
    m.write(&mut buf).unwrap();
    assert_eq!(buf.get_ref().as_slice(), &[0x12, 0x34, 0xBE, 0xEF]);
    assert_eq!(WithMagic::read(&mut Cursor::new(buf.get_ref())).unwrap(), m);
    // Wrong magic is rejected by binrw (the escape hatch keeps its semantics).
    assert!(WithMagic::read(&mut Cursor::new([0x00, 0x00, 0xBE, 0xEF])).is_err());
}

// ---- capstone: every feature in one realistic header ----

fn dns_soundness(h: &DnsHeader) -> Result<(), String> {
    // A query (qr=false in opcode here is illustrative) must not carry answers.
    if matches!(h.opcode, OpCode::Query) && !h.answers.is_empty() {
        return Err("a query must not carry answer records".into());
    }
    Ok(())
}

#[wire(big, group(opcode, flags, rcode => u16), validate = dns_soundness)]
#[derive(Debug, Clone, PartialEq)]
struct DnsHeader {
    id: u16,
    opcode: OpCode,
    flags: Flags,
    rcode: RCode,
    #[update(self.questions.len() as u16)]
    qdcount: u16,
    #[update(self.answers.len() as u16)]
    ancount: u16,
    #[br(count = qdcount)]
    #[builder(default)]
    questions: Vec<u16>,
    #[br(count = ancount)]
    #[builder(default)]
    answers: Vec<u16>,
    #[builder_only(default = 0)]
    parsed_at: u64, // a metadata field, never on the wire
}

#[test]
fn capstone_all_features() {
    let h = DnsHeader::builder()
        .id(0x00FF)
        .opcode(OpCode::Status)
        .flags(Flags::empty())
        .rcode(RCode::NoError)
        .questions(vec![0x1111, 0x2222])
        .answers(vec![0x3333])
        .parsed_at(42)
        .build()
        .unwrap();

    let mut buf = Cursor::new(Vec::new());
    h.write(&mut buf).unwrap();
    assert_eq!(
        buf.get_ref().as_slice(),
        &[
            0x00, 0xFF, // id
            0x20, 0x00, // group: opcode=Status(2) in high nibble
            0x00, 0x02, // qdcount (derived)
            0x00, 0x01, // ancount (derived)
            0x11, 0x11, 0x22, 0x22, // questions
            0x33, 0x33, // answers
        ]
    );

    let back = DnsHeader::read(&mut Cursor::new(buf.get_ref())).unwrap();
    assert_eq!(back.opcode, OpCode::Status);
    assert_eq!(back.questions, vec![0x1111, 0x2222]);
    assert_eq!(back.answers, vec![0x3333]);
    assert_eq!(back.parsed_at, 0); // builder-only default on read

    // Soundness: a Query carrying answers is rejected at build...
    let err = DnsHeader::builder()
        .id(1)
        .opcode(OpCode::Query)
        .flags(Flags::empty())
        .rcode(RCode::NoError)
        .answers(vec![0x4444])
        .build()
        .unwrap_err();
    assert!(matches!(err, bits::BuilderError::Invalid(_)));

    // ...but can still be constructed via the escape hatch, and parsed.
    let bad = DnsHeader::builder()
        .id(1)
        .opcode(OpCode::Query)
        .flags(Flags::empty())
        .rcode(RCode::NoError)
        .answers(vec![0x4444])
        .check_soundness(false)
        .build()
        .unwrap();
    assert_eq!(bad.answers, vec![0x4444]);
}
