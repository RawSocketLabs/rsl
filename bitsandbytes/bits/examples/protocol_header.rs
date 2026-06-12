//! Build, serialize, and parse a DNS-style message header using `bits` together
//! with `binrw` — the collapsed 16-bit opcode/flags/rcode field embeds in the
//! `#[binrw]` struct with no `map` glue.
//!
//! Run with: `cargo run -p bits --example protocol_header`

use binrw::{BinRead, BinWrite, binrw, io::Cursor};
use bits::{BitEnum, bitfield, u3, u4};

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[bit_enum(u4)]
enum Op {
    #[default]
    Query,
    Inverse,
    Status,
    #[catch_all]
    Other(u4),
}

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[bit_enum(u4)]
enum RCode {
    #[default]
    NoError,
    FormErr,
    ServFail,
    NxDomain,
    #[catch_all]
    Other(u4),
}

// The 16-bit collapsed field: MSB-first packing, big-endian on the wire.
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Flags {
    response: bool,
    op: Op,
    authoritative: bool,
    truncated: bool,
    recursion_desired: bool,
    recursion_available: bool,
    reserved: u3,
    rcode: RCode,
}

#[binrw]
#[brw(big)]
#[derive(Debug)]
struct Header {
    id: u16,
    flags: Flags, // <-- no #[br(map)] / #[bw(map)]
    qdcount: u16,
    ancount: u16,
    nscount: u16,
    arcount: u16,
}

fn main() {
    let header = Header {
        id: 0x1234,
        flags: Flags::new()
            .with_response(true)
            .with_op(Op::Query)
            .with_recursion_desired(true)
            .with_rcode(RCode::NoError),
        qdcount: 1,
        ancount: 0,
        nscount: 0,
        arcount: 0,
    };

    // Serialize.
    let mut buf = Cursor::new(Vec::new());
    header.write(&mut buf).unwrap();
    let bytes = buf.into_inner();
    println!("encoded header: {bytes:02x?}");
    println!("  flags word: {:#06x}", header.flags.raw());

    // Parse back.
    let parsed = Header::read(&mut Cursor::new(&bytes)).unwrap();
    println!("decoded id: {:#06x}", parsed.id);
    println!("  response: {}", parsed.flags.response());
    println!("  op: {:?}", parsed.flags.op());
    println!("  recursion_desired: {}", parsed.flags.recursion_desired());
    println!("  rcode: {:?}", parsed.flags.rcode());

    // An unknown rcode on the wire is preserved (dual-use), never rejected.
    let weird = Flags::from_raw(0x000F);
    println!("unknown rcode 0xF -> {:?}", weird.rcode());
    assert_eq!(weird.rcode(), RCode::Other(u4::new(0xF)));
}
