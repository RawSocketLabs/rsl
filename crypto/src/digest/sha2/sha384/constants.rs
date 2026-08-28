//! SHA-384 initial state.
//!
//! FIPS 180-4 §5.3.4 publishes `H_0^(0)` through `H_7^(0)` for SHA-384: the first sixty-four bits
//! of the fractional parts of the square roots of the ninth through sixteenth primes. The round
//! constants are SHA-512's (§4.2.3) and are not repeated here.

/// Eight SHA-384 initial hash words from FIPS 180-4 §5.3.4, in order.
pub(super) const INITIAL_HASH_VALUE: [u64; 8] = [
    0xcbbb_9d5d_c105_9ed8,
    0x629a_292a_367c_d507,
    0x9159_015a_3070_dd17,
    0x152f_ecd8_f70e_5939,
    0x6733_2667_ffc0_0b31,
    0x8eb4_4a87_6858_1511,
    0xdb0c_2e0d_64f9_8fa7,
    0x47b5_481d_befa_4fa4,
];

#[cfg(test)]
mod unit {
    use super::INITIAL_HASH_VALUE;

    /// Published evidence: NIST's SHA-384 example prints the same eight initial words.
    #[test]
    fn initial_words_match_the_nist_example() {
        assert_eq!(INITIAL_HASH_VALUE[0], 0xcbbb_9d5d_c105_9ed8);
        assert_eq!(INITIAL_HASH_VALUE[7], 0x47b5_481d_befa_4fa4);
    }
}
