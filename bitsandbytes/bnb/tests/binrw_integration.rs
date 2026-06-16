//! End-to-end binrw integration: `#[bitfield]` / `#[derive(BitEnum)]` types drop
//! straight into `#[binrw]` structs with **no `#[br(map)]` / `#[bw(map)]` glue**
//! — the central ergonomic win over the current
//! `modular-bitfield` + map / `bitbybit` setup.
//!
//! Requires the default `binrw` feature.
#![cfg(feature = "binrw")]

use binrw::{BinRead, BinWrite, binrw, io::Cursor};
use bnb::{BitEnum, bitfield, bitflags, u4};

// A DNS-like collapsed 16-bit field (MSB-first, big-endian): a `u4` opcode, a
// plain `u8` of flags, and a `u4` rcode enum — widths sum to 16.
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct State {
    opcode: u4,
    flags: u8,
    rcode: RCode,
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

// The headline: `state` is a bitfield embedded directly — no map attributes.
#[binrw]
#[brw(big)]
#[derive(Debug, PartialEq, Eq)]
struct Header {
    id: u16,
    state: State,
    qdcount: u16,
}

#[test]
fn binrw_round_trips_a_bitfield_without_map_glue() {
    let header = Header {
        id: 0x1234,
        state: State::new()
            .with_opcode(u4::new(2))
            .with_rcode(RCode::ServFail),
        qdcount: 1,
    };

    let mut buf = Cursor::new(Vec::new());
    header.write(&mut buf).unwrap();
    let bytes = buf.into_inner();

    // id (BE) | state (BE: opcode in high nibble of byte 0) | qdcount (BE).
    assert_eq!(&bytes[0..2], &[0x12, 0x34]);
    assert_eq!(&bytes[2..4], &0x2002u16.to_be_bytes()); // opcode<<12 | rcode
    assert_eq!(&bytes[4..6], &[0x00, 0x01]);

    let read = Header::read(&mut Cursor::new(&bytes)).unwrap();
    assert_eq!(read, header);
}

// A byte-aligned BitEnum used directly as a binrw field (e.g. a message type).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
enum Kind {
    Request,
    Response,
    #[catch_all]
    Unknown(u8),
}

#[binrw]
#[brw(big)]
#[derive(Debug, PartialEq, Eq)]
struct Tagged {
    kind: Kind,
    value: u16,
}

#[test]
fn byte_aligned_bitenum_is_a_binrw_field() {
    let t = Tagged {
        kind: Kind::Response,
        value: 0xBEEF,
    };
    let mut buf = Cursor::new(Vec::new());
    t.write(&mut buf).unwrap();
    assert_eq!(buf.get_ref(), &[0x01, 0xBE, 0xEF]);

    // An unknown tag survives via the catch-all.
    let read = Tagged::read(&mut Cursor::new([0x7F, 0x00, 0x00])).unwrap();
    assert_eq!(read.kind, Kind::Unknown(0x7F));
}

// A bitfield declared little-endian, embedded in a big-endian binrw struct:
// the bitfield's *declared* byte order must win (it is intrinsic to the type).
#[bitfield(u16, bytes = le)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LittleField {
    #[bits(0..=7)]
    lo: u8,
    #[bits(8..=15)]
    hi: u8,
}

#[binrw]
#[brw(big)]
#[derive(Debug, PartialEq, Eq)]
struct MixedEndian {
    big_a: u16,
    little: LittleField,
    big_b: u16,
}

#[test]
fn bitfield_byte_order_is_intrinsic_not_inherited() {
    let m = MixedEndian {
        big_a: 0x0102,
        little: LittleField::from_raw(0xAABB),
        big_b: 0x0304,
    };
    let mut buf = Cursor::new(Vec::new());
    m.write(&mut buf).unwrap();
    let bytes = buf.into_inner();

    assert_eq!(&bytes[0..2], &[0x01, 0x02]); // big_a: big-endian
    assert_eq!(&bytes[2..4], &[0xBB, 0xAA]); // little: little-endian, despite brw(big)
    assert_eq!(&bytes[4..6], &[0x03, 0x04]); // big_b: big-endian

    assert_eq!(MixedEndian::read(&mut Cursor::new(&bytes)).unwrap(), m);
}

// A flag set serializes directly as a binrw field — no map glue.
#[bitflags(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LinkFlags {
    up: bool,
    broadcast: bool,
    loopback: bool,
    multicast: bool,
}

#[binrw]
#[brw(big)]
#[derive(Debug, PartialEq, Eq)]
struct Interface {
    index: u16,
    flags: LinkFlags,
    mtu: u16,
}

#[test]
fn bitflags_is_a_binrw_field() {
    let iface = Interface {
        index: 2,
        flags: LinkFlags::UP | LinkFlags::MULTICAST,
        mtu: 1500,
    };
    let mut buf = Cursor::new(Vec::new());
    iface.write(&mut buf).unwrap();
    let bytes = buf.into_inner();
    assert_eq!(bytes[2], 0b0000_1001); // up (bit 0) | multicast (bit 3)
    assert_eq!(Interface::read(&mut Cursor::new(&bytes)).unwrap(), iface);
}
