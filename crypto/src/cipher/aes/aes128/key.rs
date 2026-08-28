//! Secret round-key representation for AES-128.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 197 §3.1][fips-197] fixes an AES-128 key at 128 bits. Section 3.5, equation 3.8,
//! defines a block as four words and maps state column `c` to word `v[c]`, with row `r` becoming
//! byte `r` of that word. Section 5 defines a round key as the same four-word, sixteen-byte shape.
//!
//! This module owns that representation and its destruction only. The key-schedule layer derives
//! eleven values of this shape through §5.2 `KEYEXPANSION()`; the state-transform layer consumes
//! each value through §5.1.4 `ADDROUNDKEY()`.
//!
//! Round-key bytes are never `Clone`, `Copy`, formatted, or implicitly exposed. The internal copy
//! is zeroized on drop. The caller continues to own and manage the lifetime of the original key
//! bytes passed to [`super::key_schedule::KeySchedule::expand`].
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

use zeroize::Zeroize;

#[cfg(test)]
use super::state::BLOCK_LEN;

/// Number of 32-bit words in an AES round key.
///
/// FIPS 197 §2.3 fixes `Nb = 4`; §5 describes every round key as four words.
const WORDS_PER_ROUND_KEY: usize = 4;

/// Number of bytes in one FIPS 197 word.
const WORD_BYTES: usize = 4;

/// One secret AES round key represented as four words of four bytes each.
///
/// `words[column][row]` is arranged deliberately: FIPS 197 equation 3.8 makes the word index the
/// state column and the byte-within-word index the state row. This lets `ADDROUNDKEY()` visibly
/// combine state `s[row, column]` with `words[column][row]`.
pub(super) struct RoundKey {
    words: [[u8; WORD_BYTES]; WORDS_PER_ROUND_KEY],
}

impl RoundKey {
    /// Copy one 128-bit key-shaped block into four consecutive words.
    ///
    /// **Standard mapping:** FIPS 197 §3.5 treats bytes zero through three as word zero, bytes
    /// four through seven as word one, and so on. `word_index * 4 + byte_index` expresses that
    /// mapping without integer reinterpretation or host-endian behavior.
    #[cfg(test)]
    #[must_use]
    pub(super) fn from_block(key: &[u8; BLOCK_LEN]) -> Self {
        let words = core::array::from_fn(|word_index| {
            core::array::from_fn(|byte_index| key[word_index * WORD_BYTES + byte_index])
        });

        Self { words }
    }

    /// Take ownership of four already expanded key-schedule words.
    ///
    /// **Standard mapping:** FIPS 197 §5.1.4 defines a round key as the consecutive four-word
    /// slice `w[4 * round..4 * round + 3]`. [`super::key_schedule::KeySchedule`] uses this
    /// constructor to give the state transform a distinct, zeroizing copy of exactly that slice.
    #[must_use]
    pub(super) fn from_words(words: [[u8; WORD_BYTES]; WORDS_PER_ROUND_KEY]) -> Self {
        Self { words }
    }

    /// Borrow one byte by FIPS word index, then byte-within-word index.
    ///
    /// `ADDROUNDKEY()` supplies the state column as `word_index` and the state row as
    /// `byte_index`, matching FIPS 197 equations 3.8 and 5.9.
    #[must_use]
    pub(super) fn byte(&self, word_index: usize, byte_index: usize) -> u8 {
        self.words[word_index][byte_index]
    }
}

impl Drop for RoundKey {
    fn drop(&mut self) {
        self.words.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::RoundKey;

    /// Published representation evidence from FIPS 197-upd1 Appendix B, `Key`.
    ///
    /// Appendix B prints the sixteen key bytes as both a sequence and four state-shaped rows.
    /// The expected words below are its sequence split according to §3.5 equation 3.8.
    #[test]
    fn appendix_b_key_bytes_map_to_four_published_columns() {
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let round_key = RoundKey::from_block(&key);

        assert_eq!(round_key.words[0], [0x2b, 0x7e, 0x15, 0x16]);
        assert_eq!(round_key.words[1], [0x28, 0xae, 0xd2, 0xa6]);
        assert_eq!(round_key.words[2], [0xab, 0xf7, 0x15, 0x88]);
        assert_eq!(round_key.words[3], [0x09, 0xcf, 0x4f, 0x3c]);
    }
}
