//! **standalone** — using `bnb` as a dependency-light bit/byte library: no external codec deps,
//! no sockets, just packing and unpacking a couple of IPv4-style header bytes from the field
//! types directly.
//!
//! Run with: `cargo run -p bitsandbytes --example standalone`

use bnb::{BitEnum, bitfield, u2, u4, u6};

/// An IPv4-style first byte: 4-bit version + 4-bit header length.
#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VersionIhl {
    version: u4,
    ihl: u4,
}

/// A 6-bit DSCP + 2-bit ECN, the IPv4 "type of service" byte.
#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

fn main() {
    // Pack with the immutable `with_*` builder.
    let vihl = VersionIhl::new()
        .with_version(u4::new(4))
        .with_ihl(u4::new(5));
    assert_eq!(vihl.to_be_bytes(), [0x45]); // the classic IPv4 first byte
    println!("version/IHL byte: {:#04x}", vihl.raw());

    let tos = Tos::new().with_dscp(u6::new(46)).with_ecn(Ecn::Ce); // EF + CE
    println!(
        "ToS byte: {:#04x} (dscp={}, ecn={:?})",
        tos.raw(),
        tos.dscp(),
        tos.ecn()
    );

    // Unpack from bytes.
    let parsed = VersionIhl::from_be_bytes([0x45]);
    assert_eq!(parsed.version(), u4::new(4));
    assert_eq!(parsed.ihl(), u4::new(5));
    println!("parsed version={}, ihl={}", parsed.version(), parsed.ihl());

    // Mutate in place.
    let mut tos = tos;
    tos.set_ecn(Ecn::NotEct);
    assert_eq!(tos.ecn(), Ecn::NotEct);
    println!("after clearing ECN: {:#04x}", tos.raw());
}
