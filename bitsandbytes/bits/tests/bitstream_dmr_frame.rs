//! Phase 1 capstone (ROADMAP exit): a **complete DMR frame** — slot type +
//! a 264-bit burst (with a 48-bit sync `BitEnum`) + a CRC payload — composed from
//! the native bit codec and round-tripping with **no binrw**. Plus proptest
//! round-trips and a golden byte vector.
//!
//! Exercises, together: sub-byte leaf fields (`u4`/`u108`), a sub-byte
//! `#[derive(BitEnum)]` (the 48-bit sync, catch-all preserving unknown patterns),
//! a `#[nested]` sub-message (the burst), and a `[u8; N]` payload.

use bits::{BitDecode, BitEncode, BitEnum, u4, u48, u108};
use proptest::prelude::*;

/// ETSI TS 102 361-1 Table 9.2 — 48-bit SYNC patterns; `Unknown` preserves any
/// other pattern losslessly (dual-use).
#[derive(BitEnum, Copy, Clone, Eq, PartialEq, Debug)]
#[bit_enum(u48)]
#[repr(u64)]
enum Sync {
    BaseStationVoice = 0x755F_D7DF_75F7,
    BaseStationData = 0xDFF5_7D75_DF5D,
    MobileStationVoice = 0x7F7D_5DD5_7DFD,
    #[catch_all]
    Unknown(u48),
}

/// A 264-bit DMR burst: `108 | 48 (sync) | 108`, none byte-aligned.
#[derive(BitDecode, BitEncode, Copy, Clone, Eq, PartialEq, Debug)]
struct Burst {
    payload_1: u108,
    sync: Sync,
    payload_2: u108,
}

/// A complete frame: a slot-type byte (color code + data type), the burst, and a
/// 2-byte CRC. 4 + 4 + 264 + 16 = 288 bits = 36 bytes.
#[derive(BitDecode, BitEncode, Copy, Clone, Eq, PartialEq, Debug)]
struct Frame {
    color_code: u4,
    data_type: u4,
    #[nested]
    burst: Burst,
    crc: [u8; 2],
}

fn frame_with(sync: Sync) -> Frame {
    Frame {
        color_code: u4::new(0x5),
        data_type: u4::new(0xA),
        burst: Burst {
            payload_1: u108::from_raw(0x0123_4567_89AB_CDEF_0123_4567),
            sync,
            payload_2: u108::from_raw(0x0FED_CBA9_8765_4321_0FED_CBA9),
        },
        crc: [0xBE, 0xEF],
    }
}

#[test]
fn frame_round_trips_with_no_binrw() {
    let frame = frame_with(Sync::BaseStationData);
    let bytes = frame.to_bytes().unwrap();
    assert_eq!(bytes.len(), 36, "288 bits");
    assert_eq!(Frame::decode_exact(&bytes).unwrap(), frame);
}

#[test]
fn recognizes_a_known_sync_and_preserves_unknown() {
    // A known Table 9.2 sync survives the round-trip as its named variant.
    let known = frame_with(Sync::MobileStationVoice);
    assert_eq!(
        Frame::decode_exact(&known.to_bytes().unwrap())
            .unwrap()
            .burst
            .sync,
        Sync::MobileStationVoice
    );

    // An off-table sync is preserved losslessly (dual-use), not rejected.
    let bogus = frame_with(Sync::Unknown(u48::from_raw(0x0000_0000_0001)));
    let decoded = Frame::decode_exact(&bogus.to_bytes().unwrap()).unwrap();
    assert_eq!(
        decoded.burst.sync,
        Sync::Unknown(u48::from_raw(0x0000_0000_0001))
    );
}

#[test]
fn golden_bytes() {
    // A fixed frame encodes to a fixed 36-byte vector (regression guard).
    let frame = frame_with(Sync::BaseStationVoice);
    let bytes = frame.to_bytes().unwrap();
    let golden: [u8; 36] = [
        0x5a, 0x00, 0x00, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x77,
        0x55, 0xfd, 0x7d, 0xf7, 0x5f, 0x70, 0x00, 0x0f, 0xed, 0xcb, 0xa9, 0x87, 0x65, 0x43, 0x21,
        0x0f, 0xed, 0xcb, 0xa9, 0xbe, 0xef,
    ];
    assert_eq!(bytes, golden, "got {bytes:02x?}");
    // ...and the golden bytes decode back to the frame.
    assert_eq!(Frame::decode_exact(&golden).unwrap(), frame);
}

proptest! {
    /// encode ∘ decode = id over random field values.
    #[test]
    fn frame_round_trips_prop(
        cc in any::<u8>(),
        dt in any::<u8>(),
        p1 in any::<u128>(),
        p2 in any::<u128>(),
        sync_raw in any::<u64>(),
        crc in any::<[u8; 2]>(),
    ) {
        let frame = Frame {
            color_code: u4::from_raw(cc),
            data_type: u4::from_raw(dt),
            burst: Burst {
                payload_1: u108::from_raw(p1),
                sync: <Sync as bits::Bits>::from_bits(u128::from(sync_raw)),
                payload_2: u108::from_raw(p2),
            },
            crc,
        };
        let bytes = frame.to_bytes().unwrap();
        prop_assert_eq!(bytes.len(), 36);
        prop_assert_eq!(Frame::decode_exact(&bytes).unwrap(), frame);
    }
}
