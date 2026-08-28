//! X25519 scalar preparation and fixed-position bit access.
//!
//! ## Standards ownership
//!
//! [RFC 7748 §5][rfc-7748] decodes a 32-byte X25519 scalar by clearing the low three bits,
//! clearing bit 255, setting bit 254, and interpreting the result little-endian. This module
//! keeps that prepared value in a distinct zeroizing owner and exposes only the ladder's
//! fixed-index bit reads.
//!
//! [rfc-7748]: https://www.rfc-editor.org/rfc/rfc7748.html

use crate::SecretBytes;

/// One RFC 7748-prepared X25519 scalar.
pub(super) struct PreparedScalar {
    bytes: SecretBytes<32>,
}

impl PreparedScalar {
    /// Apply all three X25519 scalar bit rules to caller-owned random bytes.
    #[must_use]
    pub(super) fn new(input: &[u8; 32]) -> Self {
        let mut prepared = *input;
        prepared[0] &= 0xf8;
        prepared[31] &= 0x7f;
        prepared[31] |= 0x40;

        Self {
            bytes: SecretBytes::new(prepared),
        }
    }

    /// Read one scalar bit by its RFC little-endian integer position.
    ///
    /// The ladder calls this for every position from 254 down through zero. No bit controls loop
    /// count or memory address beyond this same public descending index.
    #[must_use]
    pub(super) fn bit(&self, bit_index: usize) -> u64 {
        u64::from((self.bytes.expose_secret()[bit_index / 8] >> (bit_index % 8)) & 1)
    }
}

#[cfg(test)]
mod unit {
    use super::PreparedScalar;

    #[test]
    fn preparation_clears_and_sets_the_exact_rfc_7748_bits() {
        let scalar = PreparedScalar::new(&[0xff; 32]);

        assert_eq!(scalar.bytes.expose_secret()[0], 0xf8);
        assert_eq!(scalar.bytes.expose_secret()[31], 0x7f);
        assert_eq!(scalar.bit(0), 0);
        assert_eq!(scalar.bit(1), 0);
        assert_eq!(scalar.bit(2), 0);
        assert_eq!(scalar.bit(254), 1);
    }

    #[test]
    fn preparation_sets_bit_254_even_for_an_all_zero_input() {
        let scalar = PreparedScalar::new(&[0; 32]);

        assert_eq!(scalar.bytes.expose_secret()[0], 0);
        assert_eq!(scalar.bytes.expose_secret()[31], 0x40);
        assert_eq!(scalar.bit(254), 1);
        assert_eq!(scalar.bit(255), 0);
    }
}
