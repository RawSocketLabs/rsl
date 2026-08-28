//! SHA-512 block parsing and message-schedule expansion.
//!
//! FIPS 180-4 §§5.2.2 and 6.4.2 parse sixteen big-endian 64-bit words and extend them to
//! `W_0..W_79`. Every addition is explicitly modulo `2^64` through `wrapping_add`.

use super::functions::{small_sigma_0, small_sigma_1};

/// Bytes in one 1024-bit SHA-512 block.
pub(super) const BLOCK_LEN: usize = 128;

/// Words in the complete SHA-512 schedule.
pub(super) const SCHEDULE_WORDS: usize = 80;

/// Parse the first sixteen schedule words in big-endian byte order.
#[must_use]
fn parse_block(block: &[u8; BLOCK_LEN]) -> [u64; 16] {
    core::array::from_fn(|word_index| {
        let start = word_index * 8;
        u64::from_be_bytes(
            block[start..start + 8]
                .try_into()
                .expect("a SHA-512 word always selects eight bytes"),
        )
    })
}

/// Apply FIPS 180-4 §6.4.2's recurrence for schedule index `t`.
#[must_use]
fn expanded_word(schedule: &[u64; SCHEDULE_WORDS], t: usize) -> u64 {
    small_sigma_1(schedule[t - 2])
        .wrapping_add(schedule[t - 7])
        .wrapping_add(small_sigma_0(schedule[t - 15]))
        .wrapping_add(schedule[t - 16])
}

/// Construct all eighty words for one complete block.
#[must_use]
pub(super) fn build_schedule(block: &[u8; BLOCK_LEN]) -> [u64; SCHEDULE_WORDS] {
    let parsed = parse_block(block);
    let mut schedule = [0_u64; SCHEDULE_WORDS];
    schedule[..16].copy_from_slice(&parsed);
    for t in 16..SCHEDULE_WORDS {
        schedule[t] = expanded_word(&schedule, t);
    }
    schedule
}

#[cfg(test)]
mod unit {
    use super::*;

    /// Standard-derived byte-order evidence for FIPS 180-4 §§3.1 and 5.2.2.
    #[test]
    fn parsing_preserves_big_endian_word_order() {
        let block = core::array::from_fn(|index| {
            u8::try_from(index).expect("every SHA-512 block byte index fits u8")
        });
        let words = parse_block(&block);
        assert_eq!(words[0], 0x0001_0203_0405_0607);
        assert_eq!(words[15], 0x7879_7a7b_7c7d_7e7f);
    }

    /// Standard-derived recurrence evidence over a padded `abc` block.
    #[test]
    fn first_expanded_abc_words_match_the_recurrence() {
        let mut block = [0_u8; BLOCK_LEN];
        block[..4].copy_from_slice(&[b'a', b'b', b'c', 0x80]);
        block[127] = 24;
        let schedule = build_schedule(&block);
        assert_eq!(schedule[0], 0x6162_6380_0000_0000);
        assert_eq!(schedule[15], 24);
        assert_eq!(schedule[16], 0x6162_6380_0000_0000);
        assert_eq!(schedule[17], 0x0003_0000_0000_00c0);
    }
}
