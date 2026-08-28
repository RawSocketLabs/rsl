//! The six SHA-512 functions from FIPS 180-4 §4.1.3.
//!
//! A SHA-512 word is `u64`. Rust bitwise operators map directly to the standard, while
//! `rotate_right` and unsigned `>>` keep rotations distinct from logical shifts.

/// FIPS 180-4 equation 4.8, `Ch(x,y,z)`.
#[must_use]
pub(super) const fn choose(x: u64, y: u64, z: u64) -> u64 {
    (x & y) ^ (!x & z)
}

/// FIPS 180-4 equation 4.9, `Maj(x,y,z)`.
#[must_use]
pub(super) const fn majority(x: u64, y: u64, z: u64) -> u64 {
    (x & y) ^ (x & z) ^ (y & z)
}

/// FIPS 180-4 equation 4.10, `ROTR^28 ^ ROTR^34 ^ ROTR^39`.
#[must_use]
pub(super) const fn big_sigma_0(x: u64) -> u64 {
    x.rotate_right(28) ^ x.rotate_right(34) ^ x.rotate_right(39)
}

/// FIPS 180-4 equation 4.11, `ROTR^14 ^ ROTR^18 ^ ROTR^41`.
#[must_use]
pub(super) const fn big_sigma_1(x: u64) -> u64 {
    x.rotate_right(14) ^ x.rotate_right(18) ^ x.rotate_right(41)
}

/// FIPS 180-4 equation 4.12, `ROTR^1 ^ ROTR^8 ^ SHR^7`.
#[must_use]
pub(super) const fn small_sigma_0(x: u64) -> u64 {
    x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7)
}

/// FIPS 180-4 equation 4.13, `ROTR^19 ^ ROTR^61 ^ SHR^6`.
#[must_use]
pub(super) const fn small_sigma_1(x: u64) -> u64 {
    x.rotate_right(19) ^ x.rotate_right(61) ^ (x >> 6)
}

#[cfg(test)]
mod unit {
    use super::*;

    /// Standard-derived truth-table evidence for the two three-input Boolean functions.
    #[test]
    fn choose_and_majority_match_one_bit_truth() {
        let zero = 0_u64;
        let one = u64::MAX;
        assert_eq!(choose(zero, one, zero), zero);
        assert_eq!(choose(one, one, zero), one);
        assert_eq!(majority(one, zero, one), one);
        assert_eq!(majority(one, zero, zero), zero);
    }

    /// Standard-derived evidence that each published rotation or shift position is represented.
    #[test]
    fn sigma_functions_move_the_low_bit_to_expected_positions() {
        let low = 1_u64;
        assert_eq!(
            big_sigma_0(low),
            (1_u64 << 36) | (1_u64 << 30) | (1_u64 << 25)
        );
        assert_eq!(
            big_sigma_1(low),
            (1_u64 << 50) | (1_u64 << 46) | (1_u64 << 23)
        );
        assert_eq!(small_sigma_0(low), (1_u64 << 63) | (1_u64 << 56));
        assert_eq!(small_sigma_1(low), (1_u64 << 45) | (1_u64 << 3));
    }
}
