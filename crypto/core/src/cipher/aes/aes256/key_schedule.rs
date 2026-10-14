//! AES-256 key expansion.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 197 §5.2][fips-197] Algorithm 2 with `Nk = 8` and `Nr = 14` expands a 32-byte key
//! into 60 words. Two rules differ from AES-128: the round constant and `SUBWORD(ROTWORD())` are
//! applied when `i` is a multiple of eight, and — the AES-256-only branch — a plain `SUBWORD()`
//! is applied when `i mod 8 = 4`. Appendix A.3 publishes every intermediate for one key, and the
//! white-box test checks all 60 words against it.
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

use zeroize::Zeroize;

use crate::cipher::aes::aes128::{
    key::{RoundKey, RoundKeySource},
    key_schedule::{ROUND_CONSTANTS, Word, add_words, rotate_word, substitute_word},
};

const WORD_BYTES: usize = 4;
/// `Nk`, the number of 32-bit words in an AES-256 key.
const KEY_WORDS: usize = 8;
/// `Nr`, the number of rounds, from FIPS 197 Table 3.
pub(super) const ROUND_COUNT: usize = 14;
const ROUND_KEY_COUNT: usize = ROUND_COUNT + 1;
/// `Nb · (Nr + 1) = 60` expanded words.
const EXPANDED_WORDS: usize = 4 * ROUND_KEY_COUNT;
/// Bytes in an AES-256 key.
pub(super) const KEY_LEN: usize = 32;

/// The complete secret AES-256 key schedule: 60 words in Algorithm 2 order.
pub(super) struct KeySchedule {
    words: [Word; EXPANDED_WORDS],
}

impl KeySchedule {
    /// FIPS 197 §5.2 Algorithm 2 for `Nk = 8`.
    #[must_use]
    pub(super) fn expand(key: &[u8; KEY_LEN]) -> Self {
        let mut words = [[0_u8; WORD_BYTES]; EXPANDED_WORDS];

        for (word_index, word) in words[..KEY_WORDS].iter_mut().enumerate() {
            let first = word_index * WORD_BYTES;
            word.copy_from_slice(&key[first..first + WORD_BYTES]);
        }

        for word_index in KEY_WORDS..EXPANDED_WORDS {
            let mut temporary = words[word_index - 1];

            if word_index % KEY_WORDS == 0 {
                temporary = substitute_word(rotate_word(temporary));
                temporary = add_words(temporary, ROUND_CONSTANTS[word_index / KEY_WORDS - 1]);
            } else if word_index % KEY_WORDS == 4 {
                // Algorithm 2's AES-256-only branch: SUBWORD without rotation or a constant.
                temporary = substitute_word(temporary);
            }

            let earlier = words[word_index - KEY_WORDS];
            let mut expanded = add_words(earlier, temporary);
            words[word_index].copy_from_slice(&expanded);
            expanded.zeroize();
        }

        Self { words }
    }
}

impl RoundKeySource for KeySchedule {
    const ROUND_COUNT: usize = ROUND_COUNT;

    fn round_key(&self, round: usize) -> RoundKey {
        let first = round * 4;
        RoundKey::from_words(core::array::from_fn(|offset| self.words[first + offset]))
    }
}

impl Drop for KeySchedule {
    fn drop(&mut self) {
        self.words.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::cipher::aes::aes256::appendix_a3::{APPENDIX_A3_KEY, APPENDIX_A3_WORDS};

    /// Published evidence: all 60 words of FIPS 197-upd1 Appendix A.3.
    #[test]
    fn complete_key_expansion_matches_all_60_appendix_a3_words() {
        let schedule = KeySchedule::expand(&APPENDIX_A3_KEY);
        for (index, expected) in APPENDIX_A3_WORDS.iter().enumerate() {
            assert_eq!(&schedule.words[index], expected, "w[{index}]");
        }
    }

    /// Published evidence: round keys 0 and 14 are the first and last four-word groups.
    #[test]
    fn round_keys_select_consecutive_four_word_groups() {
        let schedule = KeySchedule::expand(&APPENDIX_A3_KEY);
        let first = schedule.round_key(0);
        let last = schedule.round_key(14);
        assert_eq!(first.byte(0, 0), 0x60);
        assert_eq!(last.byte(3, 3), APPENDIX_A3_WORDS[59][3]);
    }
}
