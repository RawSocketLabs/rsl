//! X448 scalar preparation (`decodeScalar448`) and fixed-position bit access.
//!
//! ## Standards ownership
//!
//! [RFC 7748 §5][rfc-7748] decodes a 56-byte X448 scalar by clearing the two least significant
//! bits of the first byte and setting the most significant bit of the last byte, then reading
//! the result little-endian.
//!
//! [rfc-7748]: https://www.rfc-editor.org/rfc/rfc7748.html

use crate::SecretBytes;

/// One RFC 7748-prepared X448 scalar.
pub(super) struct PreparedScalar {
    bytes: SecretBytes<56>,
}

impl PreparedScalar {
    /// `decodeScalar448`: clear bits 0–1, set bit 447.
    #[must_use]
    pub(super) fn new(input: &[u8; 56]) -> Self {
        let mut prepared = *input;
        prepared[0] &= 0xfc;
        prepared[55] |= 0x80;
        Self {
            bytes: SecretBytes::new(prepared),
        }
    }

    /// Read scalar bit `bit_index` (RFC little-endian integer position).
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
        let scalar = PreparedScalar::new(&[0xff; 56]);
        assert_eq!(scalar.bit(0), 0);
        assert_eq!(scalar.bit(1), 0);
        assert_eq!(scalar.bit(2), 1);
        assert_eq!(scalar.bit(447), 1);
        let zero = PreparedScalar::new(&[0; 56]);
        assert_eq!(zero.bit(447), 1);
        assert_eq!(zero.bit(446), 0);
    }
}
