//! Incremental SHA-384 state, padding, and truncated output.
//!
//! FIPS 180-4 §6.5: SHA-384 preprocesses and compresses exactly like SHA-512 (§5.1.2, §6.4) but
//! starts from the §5.3.4 initial words and outputs only the leftmost 384 bits, `H_0 || … || H_5`.
//! The two discarded words `H_6` and `H_7` are still computed; a white-box test checks them
//! against NIST's printed values.

use core::fmt;
use zeroize::Zeroize;

use crate::{
    CryptoError, Result,
    digest::{
        Digest,
        sha2::sha512::{BLOCK_LEN, FinalBlocks, compress_block, final_blocks},
    },
};

use super::constants::INITIAL_HASH_VALUE;

/// Bytes in the SHA-384 output: six of the eight 64-bit chaining words.
const DIGEST_LEN: usize = 48;

/// Largest byte-aligned message length representable by the shared 128-bit bit count.
const MAX_MESSAGE_LEN_BYTES: u128 = u128::MAX / 8;

/// A finalized 384-bit SHA-384 digest.
///
/// See the [`SHA-384 teaching page`](crate::digest::sha2::sha384) for a runnable example.
#[derive(Clone, Eq, Hash, PartialEq)]
#[must_use = "a SHA-384 digest should be compared, stored, or otherwise consumed"]
pub struct Sha384Digest([u8; DIGEST_LEN]);

impl Sha384Digest {
    /// Serialized digest length.
    pub const LEN: usize = DIGEST_LEN;

    /// Borrow all 48 digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIGEST_LEN] {
        &self.0
    }

    /// Consume the digest into its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; DIGEST_LEN] {
        self.0
    }
}

impl AsRef<[u8]> for Sha384Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Sha384Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Sha384Digest(")?;
        for byte in &self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

/// An incremental SHA-384 computation.
///
/// # Examples
///
/// ```
/// use rsl_crypto::digest::sha2::sha384::Sha384;
///
/// let mut state = Sha384::new();
/// state.update("ab")?;
/// state.update("c")?;
/// assert_eq!(state.finalize(), Sha384::digest("abc")?);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub struct Sha384 {
    chaining_value: [u64; 8],
    buffer: [u8; BLOCK_LEN],
    buffer_len: usize,
    message_len_bytes: u128,
}

impl Sha384 {
    /// Start from FIPS 180-4 §5.3.4's initial value.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            chaining_value: INITIAL_HASH_VALUE,
            buffer: [0; BLOCK_LEN],
            buffer_len: 0,
            message_len_bytes: 0,
        }
    }

    /// Incorporate existing bytes without allocating.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] before mutation if the 128-bit bit-length field
    /// cannot represent the resulting message.
    pub fn update(&mut self, input: impl AsRef<[u8]>) -> Result<()> {
        self.update_bytes(input.as_ref())
    }

    /// Apply SHA-512 padding, compress, and return the leftmost six words.
    pub fn finalize(mut self) -> Sha384Digest {
        self.finish_compression();
        let mut output = [0_u8; DIGEST_LEN];
        for (index, word) in self.chaining_value[..6].iter().copied().enumerate() {
            output[index * 8..index * 8 + 8].copy_from_slice(&word.to_be_bytes());
        }
        Sha384Digest(output)
    }

    /// Digest one complete byte representation.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] for an unrepresentable message.
    pub fn digest(input: impl AsRef<[u8]>) -> Result<Sha384Digest> {
        let mut state = Self::new();
        state.update(input)?;
        Ok(state.finalize())
    }

    /// All eight final chaining words, including the two SHA-384 discards, for published checks.
    #[cfg(test)]
    pub(super) fn final_chaining_value(mut self) -> [u64; 8] {
        self.finish_compression();
        self.chaining_value
    }

    fn finish_compression(&mut self) {
        let padded = final_blocks(&self.buffer[..self.buffer_len], self.message_len_bytes * 8);
        match padded {
            FinalBlocks::One(block) => self.compress(&block),
            FinalBlocks::Two(first, second) => {
                self.compress(&first);
                self.compress(&second);
            }
        }
    }

    /// Validate length, compress full blocks, and retain only the final partial block.
    fn update_bytes(&mut self, input: &[u8]) -> Result<()> {
        let input_len = input.len() as u128;
        let new_len = self
            .message_len_bytes
            .checked_add(input_len)
            .filter(|length| *length <= MAX_MESSAGE_LEN_BYTES)
            .ok_or(CryptoError::MessageTooLong)?;
        let mut remaining = input;

        if self.buffer_len != 0 {
            let copied = (BLOCK_LEN - self.buffer_len).min(remaining.len());
            self.buffer[self.buffer_len..self.buffer_len + copied]
                .copy_from_slice(&remaining[..copied]);
            self.buffer_len += copied;
            remaining = &remaining[copied..];
            if self.buffer_len == BLOCK_LEN {
                let block = self.buffer;
                self.compress(&block);
                self.buffer.fill(0);
                self.buffer_len = 0;
            } else {
                self.message_len_bytes = new_len;
                return Ok(());
            }
        }

        while remaining.len() >= BLOCK_LEN {
            let (block, rest) = remaining.split_at(BLOCK_LEN);
            self.compress(
                block
                    .try_into()
                    .expect("split selects one complete SHA-512-size block"),
            );
            remaining = rest;
        }

        self.buffer[..remaining.len()].copy_from_slice(remaining);
        self.buffer_len = remaining.len();
        self.message_len_bytes = new_len;
        Ok(())
    }

    fn compress(&mut self, block: &[u8; BLOCK_LEN]) {
        self.chaining_value = compress_block(self.chaining_value, block);
    }
}

impl Default for Sha384 {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Sha384 {
    fn drop(&mut self) {
        self.chaining_value.zeroize();
        self.buffer.zeroize();
        self.buffer_len.zeroize();
        self.message_len_bytes.zeroize();
    }
}

impl Digest for Sha384 {
    type Output = Sha384Digest;
    const BLOCK_LEN: usize = BLOCK_LEN;
    const OUTPUT_LEN: usize = DIGEST_LEN;

    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &[u8]) -> Result<()> {
        self.update_bytes(input)
    }
    fn finalize(self) -> Self::Output {
        self.finalize()
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    /// Published evidence: NIST's SHA-384 "abc" example prints all eight final `H` words, of
    /// which only the first six form the digest.
    #[test]
    fn nist_one_block_example_final_words_including_the_two_discarded() {
        let mut state = Sha384::new();
        state.update("abc").unwrap();
        assert_eq!(
            state.final_chaining_value(),
            [
                0xcb00_753f_45a3_5e8b,
                0xb5a0_3d69_9ac6_5007,
                0x272c_32ab_0ede_d163,
                0x1a8b_605a_43ff_5bed,
                0x8086_072b_a1e7_cc23,
                0x58ba_eca1_34c8_25a7,
                0xa303_edfd_f3b8_9cd7,
                0x0c66_918e_ce57_ba15,
            ]
        );
    }

    /// Published evidence: NIST's two-block example prints `H_6` and `H_7` too.
    #[test]
    fn nist_two_block_example_discarded_words() {
        let mut state = Sha384::new();
        state
            .update("abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu")
            .unwrap();
        let words = state.final_chaining_value();
        assert_eq!(words[0], 0x0933_0c33_f711_47e8);
        assert_eq!(words[6], 0x1e9f_1f74_49ad_1749);
        assert_eq!(words[7], 0xff33_4559_a713_5d3a);
    }
}
