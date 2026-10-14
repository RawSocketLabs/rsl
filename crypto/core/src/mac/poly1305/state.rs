//! RFC 8439 §2.5.1 Poly1305 accumulation and finalization.
//!
//! ## Standards ownership
//!
//! §2.5.1 processes the message in 16-byte blocks: each block is read little-endian, a `0x01`
//! byte is appended (so the value gains `2^128`, or `2^(8·len)` for a short final block), it is
//! added to the accumulator, and the sum is multiplied by `r` modulo `P = 2^130 - 5`. Finally
//! `s` is added and the low 128 bits are the tag.
//!
//! ## Representation
//!
//! The accumulator is three little-endian limbs of 44, 44, and 42 bits. Multiplying by `r` uses
//! schoolbook products in `u128`; any weight of `2^132` or more folds back through
//! `2^130 ≡ 5 (mod P)`, which is why the key precomputes `20·r[1]` and `20·r[2]`
//! (`2^132 = 4·2^130 ≡ 20`). Limbs are allowed to exceed their nominal width slightly between
//! blocks; a full carry chain and one conditional subtraction happen at finalization.

use zeroize::Zeroize;

use super::key::OneTimeKey;

const MASK_44: u64 = (1 << 44) - 1;
const MASK_42: u64 = (1 << 42) - 1;

/// Bytes consumed per accumulation step.
pub(super) const BLOCK_BYTES: usize = 16;

/// The running value `Acc` of §2.5.1.
pub(super) struct Accumulator {
    h: [u64; 3],
}

impl Accumulator {
    pub(super) const fn new() -> Self {
        Self { h: [0; 3] }
    }

    /// §2.5.1: `Acc = ((Acc + Block) * r) % P` for one block of 1 to 16 bytes.
    ///
    /// The `0x01` terminator is placed after the last message byte, so a full block contributes
    /// `2^128` and a short block of `n` bytes contributes `2^(8n)`.
    pub(super) fn absorb(&mut self, key: &OneTimeKey, block: &[u8]) {
        debug_assert!(!block.is_empty() && block.len() <= BLOCK_BYTES);
        let mut padded = [0_u8; 17];
        padded[..block.len()].copy_from_slice(block);
        padded[block.len()] = 0x01;

        // Read the 17-byte value into 44/44/42-bit limbs.
        let low = u128::from_le_bytes(padded[..16].try_into().expect("16 bytes"));
        let high = u64::from(padded[16]);
        let m0 = u64::try_from(low & u128::from(MASK_44)).expect("44 bits");
        let m1 = u64::try_from((low >> 44) & u128::from(MASK_44)).expect("44 bits");
        let m2 = u64::try_from(low >> 88).expect("40 bits") | (high << 40);
        padded.zeroize();

        // Acc + Block.
        let h0 = self.h[0] + m0;
        let h1 = self.h[1] + m1;
        let h2 = self.h[2] + m2;

        // (Acc + Block) * r, with every weight >= 2^132 folded through 20 = 4 * 5.
        let [r0, r1, r2] = key.r;
        let [s1, s2] = key.r_times_20;
        let d0 = u128::from(h0) * u128::from(r0)
            + u128::from(h1) * u128::from(s2)
            + u128::from(h2) * u128::from(s1);
        let d1 = u128::from(h0) * u128::from(r1)
            + u128::from(h1) * u128::from(r0)
            + u128::from(h2) * u128::from(s2);
        let d2 = u128::from(h0) * u128::from(r2)
            + u128::from(h1) * u128::from(r1)
            + u128::from(h2) * u128::from(r0);

        // Partial carry propagation; the top limb's overflow re-enters at weight 5.
        let carry = d0 >> 44;
        let n0 = u64::try_from(d0 & u128::from(MASK_44)).expect("44 bits");
        let d1 = d1 + carry;
        let carry = d1 >> 44;
        let n1 = u64::try_from(d1 & u128::from(MASK_44)).expect("44 bits");
        let d2 = d2 + carry;
        let carry = d2 >> 42;
        let n2 = u64::try_from(d2 & u128::from(MASK_42)).expect("42 bits");
        let n0 = n0 + u64::try_from(carry * 5).expect("small carry");
        let carry = n0 >> 44;
        self.h = [n0 & MASK_44, n1 + carry, n2];
    }

    /// Fully reduce the accumulator to the unique representative below `P`.
    fn canonical_limbs(&self) -> [u64; 3] {
        // Complete the carry chain.
        let mut h0 = self.h[0];
        let mut h1 = self.h[1];
        let mut h2 = self.h[2];
        h1 += h0 >> 44;
        h0 &= MASK_44;
        h2 += h1 >> 44;
        h1 &= MASK_44;
        h0 += (h2 >> 42) * 5;
        h2 &= MASK_42;
        h1 += h0 >> 44;
        h0 &= MASK_44;
        h2 += h1 >> 44;
        h1 &= MASK_44;

        // Compute g = h + 5 - 2^130; if it is non-negative, h >= P and g is the residue.
        let mut g0 = h0 + 5;
        let mut g1 = h1 + (g0 >> 44);
        g0 &= MASK_44;
        let mut g2 = h2 + (g1 >> 44);
        g1 &= MASK_44;
        let borrowed = g2 >> 42 == 0;
        g2 &= MASK_42;
        let mask = if borrowed { 0 } else { u64::MAX };
        [
            (h0 & !mask) | (g0 & mask),
            (h1 & !mask) | (g1 & mask),
            (h2 & !mask) | (g2 & mask),
        ]
    }

    /// The reduced accumulator as a little-endian 17-byte integer, for published intermediates.
    #[cfg(test)]
    pub(super) fn to_canonical_bytes(&self) -> [u8; 17] {
        let [h0, h1, h2] = self.canonical_limbs();
        let value = u128::from(h0) | (u128::from(h1) << 44) | (u128::from(h2) << 88);
        let mut bytes = [0_u8; 17];
        bytes[..16].copy_from_slice(&value.to_le_bytes());
        bytes[16] = u8::try_from(h2 >> 40).expect("two bits above 2^128");
        bytes
    }

    /// §2.5.1: `Acc + s`, serialized as the little-endian low 128 bits.
    pub(super) fn finalize(self, key: &OneTimeKey) -> [u8; BLOCK_BYTES] {
        let [h0, h1, h2] = self.canonical_limbs();
        let low_128 = u128::from(h0) | (u128::from(h1) << 44) | (u128::from(h2) << 88);
        low_128.wrapping_add(key.s).to_le_bytes()
    }
}

impl Drop for Accumulator {
    fn drop(&mut self) {
        self.h.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn example_key() -> OneTimeKey {
        OneTimeKey::new(&[
            0x85, 0xd6, 0xbe, 0x78, 0x57, 0x55, 0x6d, 0x33, 0x7f, 0x44, 0x52, 0xfe, 0x42, 0xd5,
            0x06, 0xa8, 0x01, 0x03, 0x80, 0x8a, 0xfb, 0x0d, 0xb2, 0xfd, 0x4a, 0xbf, 0xf6, 0xaf,
            0x41, 0x49, 0xf5, 0x1b,
        ])
    }

    fn acc_hex(accumulator: &Accumulator) -> alloc::string::String {
        use core::fmt::Write as _;
        let bytes = accumulator.to_canonical_bytes();
        let mut text = alloc::string::String::new();
        for byte in bytes.iter().rev() {
            write!(text, "{byte:02x}").expect("formatting into a String cannot fail");
        }
        text.trim_start_matches('0').into()
    }

    /// Published evidence: RFC 8439 §2.5.2 accumulator after each of the three blocks and the
    /// final tag for "Cryptographic Forum Research Group".
    #[test]
    fn rfc_8439_section_2_5_2_accumulator_intermediates_and_tag() {
        let key = example_key();
        let message = b"Cryptographic Forum Research Group";
        let mut accumulator = Accumulator::new();
        accumulator.absorb(&key, &message[..16]);
        assert_eq!(acc_hex(&accumulator), "2c88c77849d64ae9147ddeb88e69c83fc");
        accumulator.absorb(&key, &message[16..32]);
        assert_eq!(acc_hex(&accumulator), "2d8adaf23b0337fa7cccfb4ea344b30de");
        accumulator.absorb(&key, &message[32..]);
        assert_eq!(acc_hex(&accumulator), "28d31b7caff946c77c8844335369d03a7");
        assert_eq!(
            accumulator.finalize(&key),
            [
                0xa8, 0x06, 0x1d, 0xc1, 0x30, 0x51, 0x36, 0xc6, 0xc2, 0x2b, 0x8b, 0xaf, 0x0c, 0x01,
                0x27, 0xa9
            ]
        );
    }
}
