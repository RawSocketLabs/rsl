//! Incremental SHA-512 state, padding, and output serialization.
//!
//! FIPS 180-4 §5.1.2 appends `1`, zeroes, and a 128-bit big-endian message length. Section 6.4
//! processes 128-byte blocks and concatenates all eight final words.

use core::fmt;
use zeroize::Zeroize;

use crate::{CryptoError, Result, digest::Digest};

use super::{compression::compress_block, constants::INITIAL_HASH_VALUE, schedule::BLOCK_LEN};

/// Bytes in the SHA-512 output.
const DIGEST_LEN: usize = 64;

/// First byte of the final 128-bit bit-length field.
const LENGTH_FIELD_START: usize = BLOCK_LEN - 16;

/// Largest byte-aligned message length representable by SHA-512's 128-bit bit count.
const MAX_MESSAGE_LEN_BYTES: u128 = u128::MAX / 8;

/// One or two blocks produced by final padding.
enum FinalBlocks {
    /// Marker and length fit in one block.
    One([u8; BLOCK_LEN]),
    /// The length needs a second block.
    Two([u8; BLOCK_LEN], [u8; BLOCK_LEN]),
}

/// Apply FIPS 180-4 §5.1.2 to the uncompressed tail.
#[must_use]
fn final_blocks(tail: &[u8], bit_length: u128) -> FinalBlocks {
    let mut first = [0_u8; BLOCK_LEN];
    first[..tail.len()].copy_from_slice(tail);
    first[tail.len()] = 0x80;

    if tail.len() < LENGTH_FIELD_START {
        first[LENGTH_FIELD_START..].copy_from_slice(&bit_length.to_be_bytes());
        FinalBlocks::One(first)
    } else {
        let mut second = [0_u8; BLOCK_LEN];
        second[LENGTH_FIELD_START..].copy_from_slice(&bit_length.to_be_bytes());
        FinalBlocks::Two(first, second)
    }
}

/// A finalized 512-bit SHA-512 digest.
///
/// See the [`SHA-512 teaching page`](crate::digest::sha2::sha512) for a runnable example.
#[derive(Clone, Eq, Hash, PartialEq)]
#[must_use = "a SHA-512 digest should be compared, stored, or otherwise consumed"]
pub struct Sha512Digest([u8; DIGEST_LEN]);

impl Sha512Digest {
    /// Serialized digest length.
    pub const LEN: usize = DIGEST_LEN;

    /// Borrow all 64 digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIGEST_LEN] {
        &self.0
    }

    /// Consume the digest into its byte representation.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; DIGEST_LEN] {
        self.0
    }
}

impl AsRef<[u8]> for Sha512Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Sha512Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Sha512Digest(")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

/// Incremental SHA-512 state.
///
/// The state is cloneable so higher-level hash constructions can checkpoint it. It zeroizes on
/// drop because those constructions may hash secret material.
/// See the [`SHA-512 teaching page`](crate::digest::sha2::sha512) for one-shot use.
#[derive(Clone)]
pub struct Sha512 {
    chaining_value: [u64; 8],
    buffer: [u8; BLOCK_LEN],
    buffer_len: usize,
    message_len_bytes: u128,
}

impl Sha512 {
    /// Start from FIPS 180-4 §5.3.5's initial value.
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
    /// Returns [`CryptoError::MessageTooLong`] before mutation if SHA-512's 128-bit bit-length
    /// field cannot represent the resulting message.
    pub fn update(&mut self, input: impl AsRef<[u8]>) -> Result<()> {
        self.update_bytes(input.as_ref())
    }

    /// Apply padding and return the typed digest.
    pub fn finalize(mut self) -> Sha512Digest {
        let padded = final_blocks(&self.buffer[..self.buffer_len], self.message_len_bytes * 8);
        match padded {
            FinalBlocks::One(block) => self.compress(&block),
            FinalBlocks::Two(first, second) => {
                self.compress(&first);
                self.compress(&second);
            }
        }

        let mut output = [0_u8; DIGEST_LEN];
        for (index, word) in self.chaining_value.iter().copied().enumerate() {
            output[index * 8..index * 8 + 8].copy_from_slice(&word.to_be_bytes());
        }
        Sha512Digest(output)
    }

    /// Digest one complete byte representation.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] for an unrepresentable message.
    pub fn digest(input: impl AsRef<[u8]>) -> Result<Sha512Digest> {
        let mut state = Self::new();
        state.update(input)?;
        Ok(state.finalize())
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
                    .expect("split selects one complete SHA-512 block"),
            );
            remaining = rest;
        }

        self.buffer[..remaining.len()].copy_from_slice(remaining);
        self.buffer_len = remaining.len();
        self.message_len_bytes = new_len;
        Ok(())
    }

    /// Compress one complete block.
    fn compress(&mut self, block: &[u8; BLOCK_LEN]) {
        self.chaining_value = compress_block(self.chaining_value, block);
    }
}

impl Default for Sha512 {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Sha512 {
    fn drop(&mut self) {
        self.chaining_value.zeroize();
        self.buffer.zeroize();
        self.buffer_len.zeroize();
        self.message_len_bytes.zeroize();
    }
}

impl Digest for Sha512 {
    type Output = Sha512Digest;
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

    /// Standard-derived padding evidence at both sides of the one-block boundary.
    #[test]
    fn padding_reserves_sixteen_length_bytes() {
        let FinalBlocks::One(one) = final_blocks(&[0x61; 111], 888) else {
            panic!()
        };
        assert_eq!(one[111], 0x80);
        assert_eq!(&one[112..], &888_u128.to_be_bytes());

        let FinalBlocks::Two(first, second) = final_blocks(&[0x61; 112], 896) else {
            panic!()
        };
        assert_eq!(first[112], 0x80);
        assert_eq!(&second[112..], &896_u128.to_be_bytes());
    }

    /// Published known-answer evidence from FIPS 180-4's `abc` example.
    #[test]
    fn hashes_abc() {
        let actual = Sha512::digest("abc").expect("three bytes fit SHA-512");
        assert_eq!(
            actual.into_bytes(),
            hex64(
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
                   2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
            )
        );
    }

    /// Convert a source-formatted 128-character hex string without an external fixture parser.
    fn hex64(input: &str) -> [u8; 64] {
        let compact: alloc::string::String = input.chars().filter(|c| !c.is_whitespace()).collect();
        core::array::from_fn(|index| {
            u8::from_str_radix(&compact[index * 2..index * 2 + 2], 16).expect("fixture is hex")
        })
    }
}
