//! Spike (DESIGN §11 DD1): `#[bitwire]` dispatches one struct across two
//! backends — binrw for the byte-aligned fields (magic, trailing CRC) and the
//! bit cursor for a sub-byte region — using one `br`/`bw`/`brw` vocabulary. The
//! whole thing is a real `binrw::BinRead`/`BinWrite`, so it composes with the
//! binrw ecosystem.
#![cfg(feature = "binrw")]

use binrw::{BinRead, BinWrite};
use bnb::{BitDecode, BitEncode, BitEnum, bitwire, u48, u108};
use std::io::Cursor;

/// A 48-bit DMR-style sync pattern (catch-all preserves unknowns).
#[derive(BitEnum, Copy, Clone, Eq, PartialEq, Debug)]
#[bit_enum(u48)]
#[repr(u64)]
enum Sync {
    BaseVoice = 0x755F_D7DF_75F7,
    BaseData = 0xDFF5_7D75_DF5D,
    #[catch_all]
    Unknown(u48),
}

/// The sub-byte region: 108 + 48 + 108 = 264 bits = 33 bytes (byte-aligned total,
/// sub-byte fields). Handled by the bit cursor.
#[derive(BitDecode, BitEncode, Copy, Clone, Eq, PartialEq, Debug)]
struct Payload {
    p1: u108,
    sync: Sync,
    p2: u108,
}

/// One framed message: binrw magic + the bit region + a binrw trailer.
#[bitwire(big)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct Burst {
    #[brw(magic = 0xCAFEu16)] // pure binrw: a 2-byte magic
    seq: u16, //              pure binrw: a byte-aligned field

    #[bits] //                dispatch to the bit codec
    payload: Payload,

    crc: u16, //              pure binrw: byte-aligned trailer
}

#[test]
fn dispatched_message_round_trips_through_binrw() {
    let burst = Burst {
        seq: 0x0102,
        payload: Payload {
            p1: u108::from_raw(0x0123_4567_89AB_CDEF_0123_4567),
            sync: Sync::BaseData,
            p2: u108::from_raw(0x0FED_CBA9_8765_4321_0FED_CBA9),
        },
        crc: 0xBEEF,
    };

    // Write via binrw (BinWrite came from #[bitwire] -> #[binrw]).
    let mut buf = Cursor::new(Vec::new());
    burst.write(&mut buf).unwrap();
    let bytes = buf.into_inner();

    // Frame size: 2 (magic) + 2 (seq) + 33 (region) + 2 (crc) = 39 bytes.
    assert_eq!(bytes.len(), 39);
    assert_eq!(&bytes[0..2], &[0xCA, 0xFE], "binrw wrote the magic");
    assert_eq!(&bytes[37..39], &[0xBE, 0xEF], "binrw wrote the trailer");

    // Read it back via binrw; the bit region decodes through the cursor.
    let decoded = Burst::read(&mut Cursor::new(&bytes)).unwrap();
    assert_eq!(decoded, burst);
}

#[test]
fn bad_magic_is_a_binrw_error() {
    // The magic is binrw's job — proves the byte-aligned path is really binrw.
    let mut bytes = vec![0xDE, 0xAD]; // wrong magic
    bytes.extend(std::iter::repeat_n(0u8, 37));
    assert!(Burst::read(&mut Cursor::new(&bytes)).is_err());
}
