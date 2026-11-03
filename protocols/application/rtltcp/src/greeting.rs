//! The server greeting: the first 12 bytes an `rtl_tcp` server sends.

use bnb::bin;

/// The 12-byte greeting an `rtl_tcp` server sends on connect, before any samples.
///
/// Fields are stored verbatim: a greeting whose magic is not `RTL0` still decodes and
/// round-trips (check it with [`Greeting::has_magic`]).
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Greeting {
    /// Protocol magic; `RTL0` ([`Greeting::MAGIC`]) from a conforming server.
    pub magic: [u8; 4],
    /// The dongle's tuner type, as the server reports it.
    pub tuner_type: u32,
    /// Number of entries in the tuner's gain table, which is the range
    /// [`Opcode::SetTunerGainByIndex`](crate::Opcode::SetTunerGainByIndex) indexes.
    pub gain_count: u32,
}

impl Greeting {
    /// The magic a conforming server sends.
    pub const MAGIC: [u8; 4] = *b"RTL0";
    /// Encoded length in bytes.
    pub const LEN: usize = 12;

    /// A conforming greeting (magic `RTL0`).
    #[must_use]
    pub const fn new(tuner_type: u32, gain_count: u32) -> Self {
        Self {
            magic: Self::MAGIC,
            tuner_type,
            gain_count,
        }
    }

    /// Whether the magic is `RTL0`.
    #[must_use]
    pub fn has_magic(&self) -> bool {
        self.magic == Self::MAGIC
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn new_sets_the_conforming_magic() {
        let greeting = Greeting::new(5, 29);
        assert!(greeting.has_magic());
        assert_eq!(greeting.tuner_type, 5);
        assert_eq!(greeting.gain_count, 29);
    }

    #[test]
    fn other_magic_is_reported_not_rejected() {
        let greeting = Greeting {
            magic: *b"NOPE",
            ..Greeting::new(0, 0)
        };
        assert!(!greeting.has_magic());
    }
}

#[cfg(test)]
mod component {
    use super::*;

    #[test]
    fn greeting_round_trips_big_endian() {
        let wire = [
            b'R', b'T', b'L', b'0', 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x1d,
        ];
        let greeting = Greeting::decode_exact(&wire).unwrap();
        assert_eq!(greeting, Greeting::new(5, 29));
        assert_eq!(greeting.to_bytes().unwrap(), wire);
        assert_eq!(wire.len(), Greeting::LEN);
    }
}
