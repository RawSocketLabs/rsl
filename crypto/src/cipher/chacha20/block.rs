//! RFC 8439 §2.3 `ChaCha20` block function.
//!
//! ## Standards ownership
//!
//! §2.3 lays out the sixteen-word state as four constant words, eight key words, one block
//! counter, and three nonce words, all read little-endian. It then runs ten "double rounds"
//! (a column round followed by a diagonal round, twenty rounds total), adds the original state
//! word by word, and serializes the result little-endian into 64 keystream bytes.
//!
//! The state is a secret-bearing owner: it contains key words and is zeroized on drop.

use zeroize::Zeroize;

use super::quarter_round::{STATE_WORDS, quarter_round_on_state};

/// §2.3's constant words: the ASCII of `"expand 32-byte k"` read as little-endian words.
const CONSTANTS: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

/// Number of key bytes.
pub(super) const KEY_BYTES: usize = 32;
/// Number of nonce bytes in the IETF profile.
pub(super) const NONCE_BYTES: usize = 12;
/// Number of keystream bytes produced by one block function invocation.
pub(super) const BLOCK_BYTES: usize = 64;

/// One sixteen-word `ChaCha` state.
pub(super) struct State {
    words: [u32; STATE_WORDS],
}

impl State {
    /// §2.3 state setup: constants, key, block counter, and nonce in printed positions 0–15.
    pub(super) fn new(key: &[u8; KEY_BYTES], counter: u32, nonce: &[u8; NONCE_BYTES]) -> Self {
        let mut words = [0_u32; STATE_WORDS];
        words[..4].copy_from_slice(&CONSTANTS);
        for (index, chunk) in key.chunks_exact(4).enumerate() {
            words[4 + index] = u32::from_le_bytes(chunk.try_into().expect("four key bytes"));
        }
        words[12] = counter;
        for (index, chunk) in nonce.chunks_exact(4).enumerate() {
            words[13 + index] = u32::from_le_bytes(chunk.try_into().expect("four nonce bytes"));
        }
        Self { words }
    }

    /// §2.3 inner block: one column round then one diagonal round.
    fn double_round(&mut self) {
        // Column round.
        quarter_round_on_state(&mut self.words, 0, 4, 8, 12);
        quarter_round_on_state(&mut self.words, 1, 5, 9, 13);
        quarter_round_on_state(&mut self.words, 2, 6, 10, 14);
        quarter_round_on_state(&mut self.words, 3, 7, 11, 15);
        // Diagonal round.
        quarter_round_on_state(&mut self.words, 0, 5, 10, 15);
        quarter_round_on_state(&mut self.words, 1, 6, 11, 12);
        quarter_round_on_state(&mut self.words, 2, 7, 8, 13);
        quarter_round_on_state(&mut self.words, 3, 4, 9, 14);
    }

    /// §2.3 block function: twenty rounds, feed-forward addition, little-endian serialization.
    pub(super) fn keystream_block(&self) -> [u8; BLOCK_BYTES] {
        let mut working = State { words: self.words };
        for _ in 0..10 {
            working.double_round();
        }
        let mut output = [0_u8; BLOCK_BYTES];
        for (index, (initial, worked)) in self.words.iter().zip(working.words.iter()).enumerate() {
            let word = initial.wrapping_add(*worked);
            output[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        output
    }

    /// The state after twenty rounds without feed-forward, for published intermediate checks.
    #[cfg(test)]
    pub(super) fn after_twenty_rounds(&self) -> [u32; STATE_WORDS] {
        let mut working = State { words: self.words };
        for _ in 0..10 {
            working.double_round();
        }
        working.words
    }

    /// Borrow the words, for published setup checks.
    #[cfg(test)]
    pub(super) fn words(&self) -> &[u32; STATE_WORDS] {
        &self.words
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.words.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn sequential_key() -> [u8; 32] {
        core::array::from_fn(|index| u8::try_from(index).unwrap())
    }

    /// Published evidence: RFC 8439 §2.3.2 state setup, state after 20 rounds, and serialized
    /// block for key `00..1f`, nonce `00:00:00:09:00:00:00:4a:00:00:00:00`, counter 1.
    #[test]
    fn rfc_8439_section_2_3_2_block_function_intermediates() {
        let nonce = [0, 0, 0, 9, 0, 0, 0, 0x4a, 0, 0, 0, 0];
        let state = State::new(&sequential_key(), 1, &nonce);
        assert_eq!(
            state.words(),
            &[
                0x6170_7865,
                0x3320_646e,
                0x7962_2d32,
                0x6b20_6574,
                0x0302_0100,
                0x0706_0504,
                0x0b0a_0908,
                0x0f0e_0d0c,
                0x1312_1110,
                0x1716_1514,
                0x1b1a_1918,
                0x1f1e_1d1c,
                0x0000_0001,
                0x0900_0000,
                0x4a00_0000,
                0x0000_0000,
            ]
        );
        assert_eq!(
            state.after_twenty_rounds(),
            [
                0x8377_78ab,
                0xe238_d763,
                0xa67a_e21e,
                0x5950_bb2f,
                0xc4f2_d0c7,
                0xfc62_bb2f,
                0x8fa0_18fc,
                0x3f5e_c7b7,
                0x3352_71c2,
                0xf294_89f3,
                0xeabd_a8fc,
                0x82e4_6ebd,
                0xd19c_12b4,
                0xb04e_16de,
                0x9e83_d0cb,
                0x4e3c_50a2,
            ]
        );
        let expected_block: [u8; 64] = [
            0x10, 0xf1, 0xe7, 0xe4, 0xd1, 0x3b, 0x59, 0x15, 0x50, 0x0f, 0xdd, 0x1f, 0xa3, 0x20,
            0x71, 0xc4, 0xc7, 0xd1, 0xf4, 0xc7, 0x33, 0xc0, 0x68, 0x03, 0x04, 0x22, 0xaa, 0x9a,
            0xc3, 0xd4, 0x6c, 0x4e, 0xd2, 0x82, 0x64, 0x46, 0x07, 0x9f, 0xaa, 0x09, 0x14, 0xc2,
            0xd7, 0x05, 0xd9, 0x8b, 0x02, 0xa2, 0xb5, 0x12, 0x9c, 0xd1, 0xde, 0x16, 0x4e, 0xb9,
            0xcb, 0xd0, 0x83, 0xe8, 0xa2, 0x50, 0x3c, 0x4e,
        ];
        assert_eq!(state.keystream_block(), expected_block);
    }

    /// Published evidence: RFC 8439 §2.4.2's two block states after the block operation.
    #[test]
    fn rfc_8439_section_2_4_2_consecutive_counter_blocks() {
        let nonce = [0, 0, 0, 0, 0, 0, 0, 0x4a, 0, 0, 0, 0];
        let first = State::new(&sequential_key(), 1, &nonce);
        let second = State::new(&sequential_key(), 2, &nonce);
        assert_eq!(first.words()[12], 1);
        assert_eq!(second.words()[12], 2);
        let first_block = first.keystream_block();
        assert_eq!(
            &first_block[..8],
            &[0x22, 0x4f, 0x51, 0xf3, 0x40, 0x1b, 0xd9, 0xe1]
        );
        let first_words: [u32; 16] = core::array::from_fn(|index| {
            u32::from_le_bytes(first_block[index * 4..index * 4 + 4].try_into().unwrap())
        });
        assert_eq!(first_words[0], 0xf351_4f22);
        assert_eq!(first_words[15], 0xb741_7df0);
        let second_block = second.keystream_block();
        assert_eq!(
            u32::from_le_bytes(second_block[..4].try_into().unwrap()),
            0x9f74_a669
        );
    }
}
