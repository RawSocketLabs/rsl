//! Spike proof: a DMR burst (ETSI TS 102 361-1 §4.2.2) through the bit-level
//! codec — the case that drove this work.
//!
//! A burst is 264 bits = 108 (payload 1) + 48 (sync) + 108 (payload 2), none of
//! which are byte-aligned. With `binrw` (a *byte* `Read+Seek`) this needs manual
//! `seek_before = SeekFrom::Current(-1)` hops and `from_be_bytes`/`>> 4` nibble
//! shuffling per field. Here the three fields are just declared at their widths:
//!
//! ```ignore
//! #[derive(BitDecode, BitEncode, ...)]
//! struct GenericBurst { p1: u108, pattern: SyncPattern, p2: u108 }
//! ```
//!
//! No seeks, no shifts, no `map`. The 48-bit sync becomes a `#[derive(BitEnum)]`
//! with a `#[catch_all]`, so an unrecognized pattern is *preserved* (dual-use)
//! instead of erroring — strictly better than the original `try_map`.

mod integration {

    use bnb::{BitDecode, BitEncode, BitEnum, BitReader, BitWriter, u48, u108};

    /// ETSI TS 102 361-1 Table 9.2 — 48-bit SYNC patterns. `Unknown` preserves any
    /// other pattern losslessly (the workspace dual-use convention).
    #[derive(BitEnum, Copy, Clone, Eq, PartialEq, Debug, Hash)]
    #[bit_enum(u48)]
    #[repr(u64)]
    enum SyncPattern {
        BaseStationVoice = 0x755F_D7DF_75F7,
        BaseStationData = 0xDFF5_7D75_DF5D,
        MobileStationVoice = 0x7F7D_5DD5_7DFD,
        MobileStationData = 0xD5D7_F77F_D757,
        MobileStationReverseChannelSync = 0x77D5_5F7D_FD77,
        DirectVoiceSlotOne = 0x5D57_7F77_57FF,
        DirectDataSlotOne = 0xF7FD_D5DD_FD55,
        DirectVoiceSlotTwo = 0x7DFF_D5F5_5D5F,
        DirectDataSlotTwo = 0xD755_7F5F_F7F5,
        Reserved = 0xDD7F_F5D7_57DD,
        #[catch_all]
        Unknown(u48),
    }

    #[derive(BitDecode, BitEncode, Copy, Clone, Eq, PartialEq, Debug)]
    struct GenericBurst {
        p1: u108,             // bits   0..108
        pattern: SyncPattern, // bits 108..156
        p2: u108,             // bits 156..264
    }

    #[test]
    fn burst_round_trips_through_the_bit_stream() {
        let burst = GenericBurst {
            p1: u108::from_raw(0x0123_4567_89AB_CDEF_0123_4567),
            pattern: SyncPattern::BaseStationData,
            p2: u108::from_raw(0x0FED_CBA9_8765_4321_0FED_CBA9),
        };

        let mut w = BitWriter::new();
        burst.bit_encode(&mut w).unwrap();
        assert_eq!(w.bit_len(), 264, "108 + 48 + 108 bits");
        let bytes = w.into_bytes();
        assert_eq!(bytes.len(), 33, "264 bits packs into exactly 33 bytes");

        let mut r = BitReader::new(&bytes);
        let decoded = GenericBurst::bit_decode(&mut r).unwrap();
        assert_eq!(decoded, burst);
        assert_eq!(r.remaining_bits(), 0);
    }

    #[test]
    fn sync_pattern_lands_at_bit_offset_108() {
        let burst = GenericBurst {
            p1: u108::from_raw(0),
            pattern: SyncPattern::MobileStationVoice,
            p2: u108::from_raw(0),
        };
        let mut w = BitWriter::new();
        burst.bit_encode(&mut w).unwrap();
        let bytes = w.into_bytes();

        // Read the 48 bits starting at bit 108 directly — they are the raw pattern.
        let mut r = BitReader::new(&bytes);
        let _skip = r.read_bits(108).unwrap();
        assert_eq!(r.read_bits(48).unwrap(), 0x7F7D_5DD5_7DFD);
    }

    #[test]
    fn unknown_sync_pattern_is_preserved_not_rejected() {
        // A burst whose sync is not a Table 9.2 value: the catch-all keeps it.
        let bogus = u48::from_raw(0x0000_0000_0001);
        let burst = GenericBurst {
            p1: u108::from_raw(7),
            pattern: SyncPattern::Unknown(bogus),
            p2: u108::from_raw(9),
        };
        let mut w = BitWriter::new();
        burst.bit_encode(&mut w).unwrap();
        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        let decoded = GenericBurst::bit_decode(&mut r).unwrap();
        assert_eq!(decoded.pattern, SyncPattern::Unknown(bogus));
        assert_eq!(decoded.p1, u108::from_raw(7));
        assert_eq!(decoded.p2, u108::from_raw(9));
    }
}
