//! The tuner information an `rtl_tcp` server reports in its 12-byte greeting.

use bnb::bin;

/// The dongle's tuner type and gain-table size, which an `rtl_tcp` server sends as its
/// 12-byte greeting on connect, before any samples: the magic `RTL0`, then two big-endian
/// words.
///
/// The magic is a codec constant: encoding writes it, and decoding anything else fails
/// with [`bnb::ErrorKind::BadMagic`].
//~ rejects rtl_tcp#greeting part="magic" reason="a handshake magic identifies the protocol; a non-RTL0 greeting is not rtl_tcp, so the magic is a codec constant"
#[bin(big, magic = u32::from_be_bytes(*b"RTL0"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TunerInfo {
    /// The dongle's tuner type, as the server reports it.
    pub tuner_type: u32,
    /// Number of entries in the tuner's gain table, which is the range
    /// [`Opcode::SetTunerGainByIndex`](crate::Opcode::SetTunerGainByIndex) indexes.
    pub gain_count: u32,
}

impl TunerInfo {
    /// Tuner information reporting `tuner_type` and `gain_count`.
    #[must_use]
    pub const fn new(tuner_type: u32, gain_count: u32) -> Self {
        Self {
            tuner_type,
            gain_count,
        }
    }
}

#[cfg(test)]
mod component {
    use super::*;

    #[test]
    fn tuner_info_round_trips_with_its_magic() {
        let wire = [
            b'R', b'T', b'L', b'0', 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x1d,
        ];
        let info = TunerInfo::decode_exact(&wire).unwrap();
        assert_eq!(info, TunerInfo::new(5, 29));
        assert_eq!(info.to_bytes().unwrap(), wire);
    }

    #[test]
    fn other_magic_is_refused() {
        let err = TunerInfo::decode_exact(b"NOPE\0\0\0\x05\0\0\0\x1d").unwrap_err();
        assert!(matches!(err.kind, bnb::ErrorKind::BadMagic { .. }), "{err}");
    }
}
