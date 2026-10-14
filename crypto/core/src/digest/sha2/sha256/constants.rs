//! SHA-256 initial state and round constants.
//!
//! ## Standards ownership
//!
//! The values below are direct transcriptions of [NIST FIPS 180-4][fips-180-4]. Section 5.3.3
//! supplies the eight initial hash words; section 4.2.2 supplies the sixty-four additive round
//! constants. This module stores published values but does not regenerate them from their
//! mathematical derivations at run time.
//!
//! ## Representation
//!
//! Each hexadecimal literal is one complete 32-bit word. Numeric separators split the eight
//! hexadecimal digits only for readability and have no effect on the value. Array position is
//! significant: index `t` of `ROUND_CONSTANTS` is `K_t`, and index `i` of `INITIAL_HASH_VALUE` is
//! `H_i^(0)` in the standard's notation.
//!
//! [fips-180-4]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf

/// The initial SHA-256 hash value, ordered from H₀⁽⁰⁾ through H₇⁽⁰⁾.
///
/// **Standard mapping:** FIPS 180-4 §5.3.3 publishes these eight words and describes their
/// derivation from the fractional parts of the square roots of the first eight prime numbers.
/// The literal values, rather than a floating-point reconstruction, are the normative inputs used
/// here.
///
/// The state layer copies these words when a new digest is constructed. They are not the `K_t`
/// constants added during the sixty-four compression rounds.
pub(super) const INITIAL_HASH_VALUE: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// The constants added during compression rounds 0 through 63, in round order.
///
/// **Standard mapping:** FIPS 180-4 §4.2.2 publishes `K_0` through `K_63` and describes their
/// derivation from the first 32 bits of the fractional parts of the cube roots of the first
/// sixty-four prime numbers. Array order is round order, so compression round `t` reads
/// `ROUND_CONSTANTS[t]`.
///
/// These words are distinct from the initial hash value used to construct a fresh digest state.
pub(super) const ROUND_CONSTANTS: [u32; 64] = [
    // K_0 through K_7.
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    // K_8 through K_15.
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    // K_16 through K_23.
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    // K_24 through K_31.
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    // K_32 through K_39.
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    // K_40 through K_47.
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    // K_48 through K_55.
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    // K_56 through K_63.
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

#[cfg(test)]
mod unit {
    use super::{INITIAL_HASH_VALUE, ROUND_CONSTANTS};

    /// Published-value evidence from NIST FIPS 180-4 §5.3.3, in published order.
    #[test]
    fn initial_hash_words_match_the_standard_individually() {
        assert_eq!(INITIAL_HASH_VALUE[0], 0x6a09_e667, "H_0^(0)");
        assert_eq!(INITIAL_HASH_VALUE[1], 0xbb67_ae85, "H_1^(0)");
        assert_eq!(INITIAL_HASH_VALUE[2], 0x3c6e_f372, "H_2^(0)");
        assert_eq!(INITIAL_HASH_VALUE[3], 0xa54f_f53a, "H_3^(0)");
        assert_eq!(INITIAL_HASH_VALUE[4], 0x510e_527f, "H_4^(0)");
        assert_eq!(INITIAL_HASH_VALUE[5], 0x9b05_688c, "H_5^(0)");
        assert_eq!(INITIAL_HASH_VALUE[6], 0x1f83_d9ab, "H_6^(0)");
        assert_eq!(INITIAL_HASH_VALUE[7], 0x5be0_cd19, "H_7^(0)");
    }

    /// Published-value evidence from NIST FIPS 180-4 §4.2.2, read left to right.
    #[test]
    fn every_round_constant_matches_the_standard_in_order() {
        let expected: [u32; 64] = [
            // K_0 through K_7.
            0x428a_2f98,
            0x7137_4491,
            0xb5c0_fbcf,
            0xe9b5_dba5,
            0x3956_c25b,
            0x59f1_11f1,
            0x923f_82a4,
            0xab1c_5ed5,
            // K_8 through K_15.
            0xd807_aa98,
            0x1283_5b01,
            0x2431_85be,
            0x550c_7dc3,
            0x72be_5d74,
            0x80de_b1fe,
            0x9bdc_06a7,
            0xc19b_f174,
            // K_16 through K_23.
            0xe49b_69c1,
            0xefbe_4786,
            0x0fc1_9dc6,
            0x240c_a1cc,
            0x2de9_2c6f,
            0x4a74_84aa,
            0x5cb0_a9dc,
            0x76f9_88da,
            // K_24 through K_31.
            0x983e_5152,
            0xa831_c66d,
            0xb003_27c8,
            0xbf59_7fc7,
            0xc6e0_0bf3,
            0xd5a7_9147,
            0x06ca_6351,
            0x1429_2967,
            // K_32 through K_39.
            0x27b7_0a85,
            0x2e1b_2138,
            0x4d2c_6dfc,
            0x5338_0d13,
            0x650a_7354,
            0x766a_0abb,
            0x81c2_c92e,
            0x9272_2c85,
            // K_40 through K_47.
            0xa2bf_e8a1,
            0xa81a_664b,
            0xc24b_8b70,
            0xc76c_51a3,
            0xd192_e819,
            0xd699_0624,
            0xf40e_3585,
            0x106a_a070,
            // K_48 through K_55.
            0x19a4_c116,
            0x1e37_6c08,
            0x2748_774c,
            0x34b0_bcb5,
            0x391c_0cb3,
            0x4ed8_aa4a,
            0x5b9c_ca4f,
            0x682e_6ff3,
            // K_56 through K_63.
            0x748f_82ee,
            0x78a5_636f,
            0x84c8_7814,
            0x8cc7_0208,
            0x90be_fffa,
            0xa450_6ceb,
            0xbef9_a3f7,
            0xc671_78f2,
        ];

        for (round, (actual, expected)) in ROUND_CONSTANTS.into_iter().zip(expected).enumerate() {
            assert_eq!(actual, expected, "K_{round}");
        }
    }
}
