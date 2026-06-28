//! **ais** — a real bit-packed wire format: an AIS (marine traffic) position report.
//!
//! AIS packs fields at arbitrary bit widths with no byte alignment — a textbook motivator for a
//! bit-level codec. This models the head of a Type 1 Position Report: `msg_type` (6 bits),
//! `repeat` (2), `mmsi` (30 — the vessel id), `nav_status` (4), and `sog` (10 — speed over ground
//! in 0.1-knot units). That is 52 bits: 6.5 bytes, not byte-aligned. MSB-first / big-endian, the
//! AIS convention. (`arbitrary_width` shows the other shape — one *very wide* enum field.)
//!
//! Run with: `cargo run -p bitsandbytes --example ais`

use bnb::{BitEnum, bin, u2, u4, u6, u10, u30};

/// AIS navigation status (4 bits); only a few of the 16 codes are named, the rest are kept.
#[derive(BitEnum, Copy, Clone, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
#[repr(u8)]
enum NavStatus {
    UnderWayUsingEngine = 0,
    AtAnchor = 1,
    Moored = 5,
    Undefined = 15,
    #[catch_all]
    Other(u4),
}

/// The head of an AIS Type 1 Position Report — 52 bits, MSB-first, big-endian.
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct PositionReport {
    msg_type: u6, // 1 for a position report
    repeat: u2,
    mmsi: u30, // the vessel's Maritime Mobile Service Identity
    nav_status: NavStatus,
    sog: u10, // speed over ground, units of 0.1 knot (1023 = not available)
}

fn main() {
    let report = PositionReport {
        msg_type: u6::new(1),
        repeat: u2::new(0),
        mmsi: u30::new(227_006_760), // a vessel's MMSI
        nav_status: NavStatus::UnderWayUsingEngine,
        sog: u10::new(74), // 7.4 knots
    };
    let bytes = report.to_bytes().unwrap();
    println!("encoded: {} bytes  {bytes:02x?}", bytes.len()); // 52 bits -> 7 bytes (4 pad bits)
    assert_eq!(PositionReport::decode_exact(&bytes).unwrap(), report);

    // an unlisted nav-status code is preserved by #[catch_all], not rejected (dual-use)
    let other = PositionReport {
        nav_status: NavStatus::Other(u4::new(7)),
        ..report.clone()
    };
    assert_eq!(
        PositionReport::decode_exact(&other.to_bytes().unwrap()).unwrap(),
        other
    );

    println!("{report:#?}");
    println!("all checks passed");
}
