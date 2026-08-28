//! RFC 8439 §2.5 Poly1305 one-time key: `r` clamping and the `s` half.
//!
//! ## Standards ownership
//!
//! §2.5 splits the 32-byte one-time key into `r` (first 16 bytes) and `s` (last 16 bytes), both
//! little-endian integers, and clamps `r` by clearing the top four bits of bytes 3, 7, 11, and 15
//! and the bottom two bits of bytes 4, 8, and 12. The clamped `r` is then stored in three 44-bit
//! limbs so a product with the 130-bit accumulator fits comfortably in `u128` arithmetic.

use zeroize::Zeroize;

/// Number of bytes in a Poly1305 one-time key.
pub(super) const KEY_BYTES: usize = 32;

/// Low 44 bits, the width of the first two limbs.
const MASK_44: u128 = (1 << 44) - 1;

/// §2.5 `clamp(r)`: `r &= 0x0ffffffc0ffffffc0ffffffc0fffffff` expressed per byte.
///
/// The RFC writes the masks in decimal (`&= 15` clears the top four bits, `&= 252` clears the
/// bottom two); they are written in hexadecimal here.
fn clamp(r: &mut [u8; 16]) {
    r[3] &= 0x0f;
    r[7] &= 0x0f;
    r[11] &= 0x0f;
    r[15] &= 0x0f;
    r[4] &= 0xfc;
    r[8] &= 0xfc;
    r[12] &= 0xfc;
}

/// The clamped multiplier `r` in radix-`2^44` limbs, plus the additive half `s`.
pub(super) struct OneTimeKey {
    /// `r` as `r[0] + r[1] * 2^44 + r[2] * 2^88`; `r[2]` has at most 40 bits.
    pub(super) r: [u64; 3],
    /// `20 * r[1]` and `20 * r[2]`, the wrapped weights used when a product crosses `2^130`.
    pub(super) r_times_20: [u64; 2],
    /// `s` as a little-endian 128-bit integer.
    pub(super) s: u128,
}

impl OneTimeKey {
    /// Split and clamp a 32-byte one-time key.
    pub(super) fn new(key: &[u8; KEY_BYTES]) -> Self {
        let mut r_bytes: [u8; 16] = key[..16].try_into().expect("r occupies 16 bytes");
        clamp(&mut r_bytes);
        let r_value = u128::from_le_bytes(r_bytes);
        r_bytes.zeroize();
        let r = [
            u64::try_from(r_value & MASK_44).expect("44-bit limb"),
            u64::try_from((r_value >> 44) & MASK_44).expect("44-bit limb"),
            u64::try_from(r_value >> 88).expect("40-bit limb"),
        ];
        Self {
            r,
            r_times_20: [r[1] * 20, r[2] * 20],
            s: u128::from_le_bytes(key[16..].try_into().expect("s occupies 16 bytes")),
        }
    }
}

impl Drop for OneTimeKey {
    fn drop(&mut self) {
        self.r.zeroize();
        self.r_times_20.zeroize();
        self.s.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    /// Published evidence: RFC 8439 §2.5.2 clamped `r` and `s` for the example key material.
    #[test]
    fn rfc_8439_section_2_5_2_clamped_r_and_s() {
        let key: [u8; 32] = [
            0x85, 0xd6, 0xbe, 0x78, 0x57, 0x55, 0x6d, 0x33, 0x7f, 0x44, 0x52, 0xfe, 0x42, 0xd5,
            0x06, 0xa8, 0x01, 0x03, 0x80, 0x8a, 0xfb, 0x0d, 0xb2, 0xfd, 0x4a, 0xbf, 0xf6, 0xaf,
            0x41, 0x49, 0xf5, 0x1b,
        ];
        let one_time = OneTimeKey::new(&key);
        let r = u128::from(one_time.r[0])
            | (u128::from(one_time.r[1]) << 44)
            | (u128::from(one_time.r[2]) << 88);
        assert_eq!(r, 0x0806_d540_0e52_447c_036d_5554_08be_d685);
        assert_eq!(one_time.s, 0x1bf5_4941_aff6_bf4a_fdb2_0dfb_8a80_0301);
    }
}
