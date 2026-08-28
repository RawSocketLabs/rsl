//! Message parsing and schedule expansion for one SHA-256 block.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 180-4 §3.1][fips-180-4] establishes the big-endian relationship between bytes and
//! words. Section 5.2.1 parses each 512-bit block as sixteen 32-bit words, and section 6.2.2
//! expands those words into the 64-word message schedule used by compression.
//!
//! This layer parses `W_0` through `W_15`, exposes one recurrence step for focused testing, and
//! constructs the complete `W_0` through `W_63` schedule. Padding remains outside this module:
//! section 5.1.1 preprocessing belongs to `state`.
//!
//! ## Representation
//!
//! A block is `[u8; 64]`, making the 512-bit size a type-level precondition. Each group of four
//! bytes becomes one `u32` through `u32::from_be_bytes`. Schedule addition uses `wrapping_add`
//! because FIPS 180-4 §3.2 and §6.2.2 require addition modulo `2^32`.
//!
//! [fips-180-4]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf

use super::functions::{small_sigma_0, small_sigma_1};

/// The length of one SHA-256 message block in bytes.
///
/// FIPS 180-4 §5.2.1 specifies a 512-bit block; 512 bits divided by 8 bits per byte is 64 bytes.
/// Keeping this value in the array type prevents this parsing layer from accepting a partial or
/// oversized block.
pub(super) const BLOCK_LEN: usize = 64;

/// The number of 32-bit words parsed directly from one message block.
///
/// FIPS 180-4 §5.2.1 assigns the sixteen directly parsed words the indices `0` through `15`.
const PARSED_WORDS: usize = 16;

/// The total number of words in one expanded SHA-256 message schedule.
///
/// FIPS 180-4 §6.2.2 defines words `W_0` through `W_63`. The compression layer consumes exactly
/// one such 64-word schedule for each 512-bit message block.
pub(super) const SCHEDULE_WORDS: usize = 64;

/// Parse one complete SHA-256 message block into its first sixteen schedule words.
///
/// **Standard mapping:** FIPS 180-4 §3.1 and §5.2.1 require each consecutive group of four bytes
/// to become one 32-bit word in big-endian order. `word_index * 4` selects the first byte of
/// `W[word_index]`; `u32::from_be_bytes` places that first byte in bits 31 through 24.
///
/// **Boundary:** the input type requires exactly one complete block. This function does not pad
/// input and does not expand the remaining forty-eight schedule words.
#[must_use]
pub(super) fn parse_block(block: &[u8; BLOCK_LEN]) -> [u32; PARSED_WORDS] {
    core::array::from_fn(|word_index| {
        let byte_index = word_index * 4;

        u32::from_be_bytes([
            block[byte_index],
            block[byte_index + 1],
            block[byte_index + 2],
            block[byte_index + 3],
        ])
    })
}

/// Calculate one expanded schedule word from the four preceding words named by the standard.
///
/// **Standard mapping:** FIPS 180-4 §6.2.2 defines, for `16 <= t <= 63`, the recurrence
/// `W_t = sigma1(W_(t-2)) + W_(t-7) + sigma0(W_(t-15)) + W_(t-16)`. The arguments use that same
/// term order, making an incorrect schedule index visible at the call site.
///
/// **Rust mapping:** each `wrapping_add` is addition modulo `2^32`, as required by §3.2. The
/// lowercase sigma functions are the equation 4.6 and 4.7 operations implemented in `functions`.
/// This helper calculates one word; [`build_schedule`] enforces the valid `t` range while
/// constructing the complete schedule.
#[must_use]
const fn expand_word(
    word_minus_2: u32,
    word_minus_7: u32,
    word_minus_15: u32,
    word_minus_16: u32,
) -> u32 {
    small_sigma_1(word_minus_2)
        .wrapping_add(word_minus_7)
        .wrapping_add(small_sigma_0(word_minus_15))
        .wrapping_add(word_minus_16)
}

/// Construct the complete 64-word message schedule for one block.
///
/// **Standard mapping:** FIPS 180-4 §6.2.2 defines `W_0` through `W_15` as the words parsed from
/// the current message block. For each subsequent index `t` from 16 through 63, it applies the
/// recurrence implemented by [`expand_word`].
///
/// **Rust mapping:** the first slice copy preserves every parsed word at the same index. The loop
/// starts at `PARSED_WORDS`, so all four earlier indices needed by the recurrence are initialized
/// before they are read. Iterating to `SCHEDULE_WORDS` fills index 63 and no later index.
#[must_use]
pub(super) fn build_schedule(block: &[u8; BLOCK_LEN]) -> [u32; SCHEDULE_WORDS] {
    let parsed_words = parse_block(block);
    let mut schedule = [0_u32; SCHEDULE_WORDS];

    schedule[..PARSED_WORDS].copy_from_slice(&parsed_words);

    for word_index in PARSED_WORDS..SCHEDULE_WORDS {
        schedule[word_index] = expand_word(
            schedule[word_index - 2],
            schedule[word_index - 7],
            schedule[word_index - 15],
            schedule[word_index - 16],
        );
    }

    schedule
}

#[cfg(test)]
mod unit {
    use super::{BLOCK_LEN, SCHEDULE_WORDS, build_schedule, expand_word, parse_block};

    /// Standard-derived fixture from the pre-padded `abc` example in FIPS 180-4 §5.1.1.
    ///
    /// The helper reproduces the one 512-bit block printed by the standard. It does not test a
    /// production padding implementation, which does not exist yet.
    fn padded_abc_block() -> [u8; BLOCK_LEN] {
        let mut block = [0_u8; BLOCK_LEN];
        block[0] = b'a';
        block[1] = b'b';
        block[2] = b'c';
        block[3] = 0x80;
        block[63] = 24;
        block
    }

    /// Standard-derived evidence from NIST FIPS 180-4 §3.1 and §5.2.1.
    #[test]
    fn parses_every_word_of_a_sequential_block_in_big_endian_order() {
        let block = core::array::from_fn(|index| {
            u8::try_from(index).expect("every 64-byte block index fits in u8")
        });

        let words = parse_block(&block);

        assert_eq!(
            words,
            [
                0x0001_0203,
                0x0405_0607,
                0x0809_0a0b,
                0x0c0d_0e0f,
                0x1011_1213,
                0x1415_1617,
                0x1819_1a1b,
                0x1c1d_1e1f,
                0x2021_2223,
                0x2425_2627,
                0x2829_2a2b,
                0x2c2d_2e2f,
                0x3031_3233,
                0x3435_3637,
                0x3839_3a3b,
                0x3c3d_3e3f,
            ]
        );
    }

    /// Published-example evidence from the pre-padded `abc` block in FIPS 180-4 §5.1.1.
    #[test]
    fn parses_the_pre_padded_abc_example_without_owning_padding() {
        let words = parse_block(&padded_abc_block());

        assert_eq!(
            words,
            [
                0x6162_6380,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0x0000_0018,
            ]
        );
    }

    /// Standard-derived integration evidence from FIPS 180-4 §5.2.1 and §6.2.2.
    ///
    /// Distinct sequential bytes make every directly parsed word observable, including its
    /// position. This checks that complete schedule construction preserves all sixteen words
    /// before expansion begins.
    #[test]
    fn complete_schedule_starts_with_every_parsed_word_at_the_same_index() {
        let block = core::array::from_fn(|index| {
            u8::try_from(index).expect("every 64-byte block index fits in u8")
        });
        let parsed_words = parse_block(&block);
        let schedule = build_schedule(&block);

        assert_eq!(&schedule[..parsed_words.len()], parsed_words.as_slice());
    }

    /// Standard-derived evidence from NIST FIPS 180-4 §4.1.2 and §6.2.2.
    #[test]
    fn each_argument_occupies_its_specified_recurrence_position() {
        assert_eq!(expand_word(1, 0, 0, 0), 0x0000_a000, "sigma-one");
        assert_eq!(expand_word(0, 0x1234_5678, 0, 0), 0x1234_5678, "W[t-7]");
        assert_eq!(expand_word(0, 0, 1, 0), 0x0200_4000, "sigma-zero");
        assert_eq!(expand_word(0, 0, 0, 0x89ab_cdef), 0x89ab_cdef, "W[t-16]");
    }

    /// Standard-derived boundary evidence from FIPS 180-4 §3.2 and §6.2.2.
    #[test]
    fn expansion_addition_wraps_at_32_bits() {
        assert_eq!(expand_word(0, u32::MAX, 0, 1), 0);
    }

    /// Standard-derived evidence combining the FIPS 180-4 §5.1.1 `abc` block with the §6.2.2
    /// recurrence.
    ///
    /// FIPS 180-4 does not publish `W_16` or `W_17` for this example. The expected values below
    /// are independently calculated consequences of the cited input and recurrence, not
    /// published known-answer vectors.
    #[test]
    fn calculates_words_16_and_17_for_the_pre_padded_abc_block() {
        let words = parse_block(&padded_abc_block());

        let word_16 = expand_word(words[14], words[9], words[1], words[0]);
        let word_17 = expand_word(words[15], words[10], words[2], words[1]);

        assert_eq!(word_16, 0x6162_6380);
        assert_eq!(word_17, 0x000f_0000);
    }

    /// Mixed published and standard-derived evidence for the complete `abc` schedule.
    ///
    /// The input and `W_0` through `W_15` are published in NIST's official *Secure Hash
    /// Algorithm — Message Digest Length = 256* one-block sample. NIST does not print expanded
    /// words `W_16` through `W_63` there; those expected words are a fixed fixture derived by
    /// applying the FIPS 180-4 §6.2.2 recurrence. The provenance distinction is recorded in
    /// `STANDARDS.md`.
    #[test]
    fn complete_abc_schedule_matches_the_published_input_and_derived_expansion() {
        let expected: [u32; SCHEDULE_WORDS] = [
            0x6162_6380,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0000,
            0x0000_0018,
            0x6162_6380,
            0x000f_0000,
            0x7da8_6405,
            0x6000_03c6,
            0x3e9d_7b78,
            0x0183_fc00,
            0x12dc_bfdb,
            0xe2e2_c38e,
            0xc821_5c1a,
            0xb736_79a2,
            0xe5bc_3909,
            0x3266_3c5b,
            0x9d20_9d67,
            0xec87_26cb,
            0x7021_38a4,
            0xd3b7_973b,
            0x93f5_997f,
            0x3b68_ba73,
            0xaff4_ffc1,
            0xf10a_5c62,
            0x0a8b_3996,
            0x72af_830a,
            0x9409_e33e,
            0x2464_1522,
            0x9f47_bf94,
            0xf0a6_4f5a,
            0x3e24_6a79,
            0x2733_3ba3,
            0x0c47_63f2,
            0x840a_bf27,
            0x7a29_0d5d,
            0x065c_43da,
            0xfb3e_89cb,
            0xcc76_17db,
            0xb9e6_6c34,
            0xa999_3667,
            0x84ba_dedd,
            0xc214_62bc,
            0x1487_472c,
            0xb20f_7a99,
            0xef57_b9cd,
            0xebe6_b238,
            0x9fe3_095e,
            0x78bc_8d4b,
            0xa43f_cf15,
            0x668b_2ff8,
            0xeeab_a2cc,
            0x12b1_edeb,
        ];

        let schedule = build_schedule(&padded_abc_block());

        for (word_index, (actual, expected)) in schedule.into_iter().zip(expected).enumerate() {
            assert_eq!(actual, expected, "W_{word_index}");
        }
    }
}
