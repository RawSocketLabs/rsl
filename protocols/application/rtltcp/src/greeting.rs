//! The server greeting: the first 12 bytes an `rtl_tcp` server sends.

use bnb::bin;

/// The 12-byte greeting an `rtl_tcp` server sends on connect, before any samples: the
/// magic `RTL0`, then two big-endian words.
///
/// The magic is a codec constant: encoding writes it, and decoding anything else fails
/// with [`bnb::ErrorKind::BadMagic`].
#[bin(big, magic = u32::from_be_bytes(*b"RTL0"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Greeting {
    /// The dongle's tuner type, as the server reports it.
    pub tuner_type: u32,
    /// Number of entries in the tuner's gain table, which is the range
    /// [`Opcode::SetTunerGainByIndex`](crate::Opcode::SetTunerGainByIndex) indexes.
    pub gain_count: u32,
}

impl Greeting {
    /// Encoded length in bytes, magic included.
    pub const LEN: usize = 12;

    /// A greeting reporting `tuner_type` and `gain_count`.
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
    fn greeting_round_trips_with_its_magic() {
        let wire = [
            b'R', b'T', b'L', b'0', 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x1d,
        ];
        let greeting = Greeting::decode_exact(&wire).unwrap();
        assert_eq!(greeting, Greeting::new(5, 29));
        assert_eq!(greeting.to_bytes().unwrap(), wire);
        assert_eq!(wire.len(), Greeting::LEN);
    }

    #[test]
    fn other_magic_is_refused() {
        let err = Greeting::decode_exact(b"NOPE\0\0\0\x05\0\0\0\x1d").unwrap_err();
        assert!(matches!(err.kind, bnb::ErrorKind::BadMagic { .. }), "{err}");
    }
}
