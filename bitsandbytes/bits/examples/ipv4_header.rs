//! A complete IPv4 header (RFC 791) built from `bits` + `binrw` — the whole
//! crate working together on a real, multi-field, multi-byte layout. Several
//! bitfields and a byte-aligned enum embed in one `#[binrw]` struct with no
//! `map` glue.
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-------+-------+---------------+-------------------------------+
//! |Version|  IHL  |Type of Service|          Total Length         |
//! +-------+-------+---------------+-----+-------------------------+
//! |         Identification        |Flags|      Fragment Offset    |
//! +---------------+---------------+-----+-------------------------+
//! |  Time to Live |    Protocol   |         Header Checksum       |
//! +---------------+---------------+-------------------------------+
//! |                       Source Address                          |
//! +---------------------------------------------------------------+
//! |                    Destination Address                        |
//! +---------------------------------------------------------------+
//! ```
//!
//! Run with: `cargo run -p bits --example ipv4_header`

use std::net::Ipv4Addr;

use binrw::{binrw, io::Cursor, BinRead, BinWrite};
use bits::{bitfield, u13, u2, u4, u6, BitEnum};

/// Byte 0: version (high nibble) and header length in 32-bit words (low nibble).
#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, Debug)]
struct VersionIhl {
    version: u4,
    ihl: u4,
}

/// Byte 1: 6-bit DSCP + 2-bit ECN.
#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, Debug)]
struct Tos {
    dscp: u6,
    ecn: Ecn,
}

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
enum Ecn {
    NotEct,
    Ect1,
    Ect0,
    Ce,
}

/// Bytes 6-7: 3 flag bits + a 13-bit fragment offset, big-endian.
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug)]
struct FlagsFragment {
    reserved: bool,
    dont_fragment: bool,
    more_fragments: bool,
    fragment_offset: u13,
}

/// Byte 9: the transport protocol, a byte-aligned enum (used directly as a
/// binrw field). Unknown protocols are preserved via the catch-all.
///
/// The IANA numbers are non-contiguous (1, 6, 17), so with a catch-all this enum
/// needs `#[repr(u8)]` + explicit discriminants (Rust's rule for explicit
/// discriminants alongside a non-unit variant).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum Protocol {
    Icmp = 1,
    Tcp = 6,
    Udp = 17,
    #[catch_all]
    Other(u8),
}

#[binrw]
#[brw(big)]
#[derive(Debug)]
struct Ipv4Header {
    version_ihl: VersionIhl,
    tos: Tos,
    total_length: u16,
    identification: u16,
    flags_fragment: FlagsFragment,
    ttl: u8,
    protocol: Protocol,
    checksum: u16,
    #[br(map = |x: u32| Ipv4Addr::from(x))]
    #[bw(map = |a: &Ipv4Addr| u32::from(*a))]
    source: Ipv4Addr,
    #[br(map = |x: u32| Ipv4Addr::from(x))]
    #[bw(map = |a: &Ipv4Addr| u32::from(*a))]
    destination: Ipv4Addr,
}

fn main() {
    let header = Ipv4Header {
        version_ihl: VersionIhl::new().with_version(u4::new(4)).with_ihl(u4::new(5)),
        tos: Tos::new().with_dscp(u6::new(0)).with_ecn(Ecn::NotEct),
        total_length: 20 + 1480,
        identification: 0x1c46,
        flags_fragment: FlagsFragment::new()
            .with_dont_fragment(true)
            .with_fragment_offset(u13::new(0)),
        ttl: 64,
        protocol: Protocol::Tcp,
        checksum: 0xB1E6,
        source: Ipv4Addr::new(192, 0, 2, 1),
        destination: Ipv4Addr::new(203, 0, 113, 7),
    };

    let mut buf = Cursor::new(Vec::new());
    header.write(&mut buf).unwrap();
    let bytes = buf.into_inner();
    println!("encoded {}-byte header:", bytes.len());
    println!("  {bytes:02x?}");
    assert_eq!(bytes[0], 0x45); // version 4, IHL 5
    assert_eq!(bytes[9], 6); // protocol = TCP

    let parsed = Ipv4Header::read(&mut Cursor::new(&bytes)).unwrap();
    println!("decoded:");
    println!("  version={}, ihl={}", parsed.version_ihl.version(), parsed.version_ihl.ihl());
    println!("  dscp={}, ecn={:?}", parsed.tos.dscp(), parsed.tos.ecn());
    println!("  DF={}, frag_off={}", parsed.flags_fragment.dont_fragment(), parsed.flags_fragment.fragment_offset());
    println!("  ttl={}, protocol={:?}", parsed.ttl, parsed.protocol);
    println!("  {} -> {}", parsed.source, parsed.destination);

    // An unknown protocol number survives via the catch-all (dual-use).
    let raw_proto = <Protocol as bits::Bits>::from_bits(0x99);
    println!("unknown protocol 0x99 -> {raw_proto:?}");
    assert_eq!(raw_proto, Protocol::Other(0x99));
}
