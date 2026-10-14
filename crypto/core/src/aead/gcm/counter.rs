//! The 32-bit incrementing function used by GCM counter mode.
//!
//! ## Standards ownership
//!
//! [NIST SP 800-38D §6.2][sp-800-38d] defines `inc_s(X)` by retaining the leftmost
//! `len(X) - s` bits and replacing the rightmost `s` bits with their integer value plus one,
//! reduced modulo `2^s`. Section 6.5 Algorithm 3 fixes `s = 32` when GCTR derives `CB_i` from
//! `CB_(i-1)`. Algorithms 4 and 5 also use `inc32(J_0)` as GCTR's initial counter block.
//!
//! This module owns only `inc32` and its exact-size counter representation. It does not define
//! the initial block `J_0`, apply AES, XOR a keystream with data, impose GCM invocation limits,
//! or advance any protocol sequence number. TLS and SSH sequence numbers remain in their protocol
//! repositories and must not be confused with this internal GCM block counter.
//!
//! ## Representation mapping
//!
//! A counter block remains sixteen bytes in displayed bit-string order. The first twelve bytes are
//! the unchanged `MSB_96(X)`. `u32::from_be_bytes` is exactly §6.1's `int(LSB_32(X))`, because the
//! leftmost of those four bytes contains the integer's most-significant bits. `wrapping_add(1)`
//! implements reduction modulo `2^32`, and `to_be_bytes` maps the result back through `[x]_32`.
//! No native-endian reinterpretation occurs.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf

use zeroize::Zeroize;

/// Number of bytes in the 128-bit GCM counter block.
const BLOCK_BYTES: usize = 16;

/// First byte belonging to §6.2's rightmost 32-bit integer field.
const COUNTER_START: usize = 12;

/// One exact-size GCM counter block.
///
/// This type is semantically distinct from AES keys, plaintext/ciphertext blocks, nonces, and
/// protocol sequence numbers. It remains private and non-`Clone`; its bytes are cleared on drop.
pub(super) struct CounterBlock {
    bytes: [u8; BLOCK_BYTES],
}

impl CounterBlock {
    /// Take ownership of one complete initial counter block without changing it.
    #[must_use]
    pub(super) fn new(bytes: [u8; BLOCK_BYTES]) -> Self {
        Self { bytes }
    }

    /// Apply SP 800-38D `inc32` once in place.
    pub(super) fn increment(&mut self) {
        let current = u32::from_be_bytes([
            self.bytes[COUNTER_START],
            self.bytes[COUNTER_START + 1],
            self.bytes[COUNTER_START + 2],
            self.bytes[COUNTER_START + 3],
        ]);
        let incremented = current.wrapping_add(1).to_be_bytes();

        self.bytes[COUNTER_START..].copy_from_slice(&incremented);
    }

    /// Borrow the complete current counter block in unchanged GCM byte order.
    #[must_use]
    pub(super) fn as_block(&self) -> &[u8; BLOCK_BYTES] {
        &self.bytes
    }
}

impl Drop for CounterBlock {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::CounterBlock;

    /// Published counter-block evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 2.
    ///
    /// The document publishes `J0 = cafebabe...00000001` and GCTR block one's
    /// `CB = cafebabe...00000002`. Algorithm 4 step 3 connects them through `inc32(J0)`.
    #[test]
    fn incrementing_published_j0_produces_the_first_published_counter_block() {
        let mut counter = CounterBlock::new([
            0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88, 0x00, 0x00,
            0x00, 0x01,
        ]);

        counter.increment();

        assert_eq!(
            counter.as_block(),
            &[
                0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88, 0x00, 0x00,
                0x00, 0x02,
            ]
        );
    }

    /// Published repeated-counter evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 2,
    /// GCTR blocks 1 through 4.
    #[test]
    fn repeated_increments_follow_all_four_published_counter_blocks() {
        let prefix = [
            0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88,
        ];
        let mut initial = [0_u8; 16];
        initial[..12].copy_from_slice(&prefix);
        initial[15] = 1;
        let mut counter = CounterBlock::new(initial);

        for published_low_byte in 2_u8..=5 {
            counter.increment();

            let mut expected = [0_u8; 16];
            expected[..12].copy_from_slice(&prefix);
            expected[15] = published_low_byte;
            assert_eq!(counter.as_block(), &expected);
        }
    }

    /// Standard-derived carry evidence from SP 800-38D §6.2's integer addition rule.
    #[test]
    fn carry_crosses_each_of_the_four_counter_bytes() {
        let mut counter = CounterBlock::new([
            0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe, 0x11, 0x22, 0x33, 0x44, 0x00, 0xff,
            0xff, 0xff,
        ]);

        counter.increment();

        assert_eq!(
            counter.as_block(),
            &[
                0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe, 0x11, 0x22, 0x33, 0x44, 0x01, 0x00,
                0x00, 0x00,
            ]
        );
    }

    /// Standard-derived modular-overflow evidence from SP 800-38D §6.2's `mod 2^32` rule.
    #[test]
    fn overflow_wraps_only_the_low_32_bits() {
        let mut counter = CounterBlock::new([
            0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe, 0x11, 0x22, 0x33, 0x44, 0xff, 0xff,
            0xff, 0xff,
        ]);

        counter.increment();

        assert_eq!(
            counter.as_block(),
            &[
                0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe, 0x11, 0x22, 0x33, 0x44, 0x00, 0x00,
                0x00, 0x00,
            ]
        );
    }
}
