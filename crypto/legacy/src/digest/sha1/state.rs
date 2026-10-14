//! SHA-1 streaming, padding, and big-endian digest serialization.

use core::fmt;
use zeroize::Zeroize;

use super::compression::{BLOCK_LEN, INITIAL_STATE, compress};
use rsl_crypto::{CryptoError, Result, digest::Digest};

const OUTPUT_LEN: usize = 20;
const LENGTH_START: usize = 56;
const MAX_BYTES: u64 = u64::MAX / 8;

/// A historical 160-bit SHA-1 digest.
#[derive(Clone, Eq, Hash, PartialEq)]
#[must_use = "a SHA-1 digest has no effect unless its bytes are examined"]
pub struct Sha1Digest([u8; OUTPUT_LEN]);

impl Sha1Digest {
    /// Encoded digest size.
    pub const LEN: usize = OUTPUT_LEN;
    /// Borrow the complete digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; OUTPUT_LEN] {
        &self.0
    }
    /// Consume the digest into bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; OUTPUT_LEN] {
        self.0
    }
}

impl AsRef<[u8]> for Sha1Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl fmt::Debug for Sha1Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sha1Digest(")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

/// Incremental SHA-1 state retained only for historical interoperability.
#[derive(Clone)]
pub struct Sha1 {
    state: [u32; 5],
    buffer: [u8; BLOCK_LEN],
    buffer_len: usize,
    message_len: u64,
}

impl Sha1 {
    /// Construct FIPS 180-4 §5.3.1's initial state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: INITIAL_STATE,
            buffer: [0; BLOCK_LEN],
            buffer_len: 0,
            message_len: 0,
        }
    }

    /// Add message bytes without allocating.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] before mutation when the bit length would reach
    /// `2^64`.
    pub fn update(&mut self, input: impl AsRef<[u8]>) -> Result<()> {
        self.update_bytes(input.as_ref())
    }

    /// Apply SHA-1 padding and return the historical digest.
    pub fn finalize(mut self) -> Sha1Digest {
        let bit_length = self.message_len * 8;
        let mut first = [0_u8; BLOCK_LEN];
        first[..self.buffer_len].copy_from_slice(&self.buffer[..self.buffer_len]);
        first[self.buffer_len] = 0x80;
        if self.buffer_len < LENGTH_START {
            first[LENGTH_START..].copy_from_slice(&bit_length.to_be_bytes());
            self.compress(&first);
        } else {
            self.compress(&first);
            let mut second = [0_u8; BLOCK_LEN];
            second[LENGTH_START..].copy_from_slice(&bit_length.to_be_bytes());
            self.compress(&second);
        }
        let mut output = [0_u8; OUTPUT_LEN];
        for (index, word) in self.state.iter().enumerate() {
            output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        Sha1Digest(output)
    }

    /// Hash one complete historical byte string.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] at SHA-1's length limit.
    pub fn digest(input: impl AsRef<[u8]>) -> Result<Sha1Digest> {
        let mut state = Self::new();
        state.update(input)?;
        Ok(state.finalize())
    }

    fn update_bytes(&mut self, input: &[u8]) -> Result<()> {
        let input_len = u64::try_from(input.len()).map_err(|_| CryptoError::MessageTooLong)?;
        let new_len = self
            .message_len
            .checked_add(input_len)
            .filter(|value| *value <= MAX_BYTES)
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
                self.message_len = new_len;
                return Ok(());
            }
        }
        while remaining.len() >= BLOCK_LEN {
            let (block, rest) = remaining.split_at(BLOCK_LEN);
            self.compress(block.try_into().expect("split selects a SHA-1 block"));
            remaining = rest;
        }
        self.buffer[..remaining.len()].copy_from_slice(remaining);
        self.buffer_len = remaining.len();
        self.message_len = new_len;
        Ok(())
    }

    fn compress(&mut self, block: &[u8; BLOCK_LEN]) {
        self.state = compress(self.state, block);
    }
}

impl Default for Sha1 {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for Sha1 {
    fn drop(&mut self) {
        self.state.zeroize();
        self.buffer.zeroize();
        self.buffer_len.zeroize();
        self.message_len.zeroize();
    }
}
impl Digest for Sha1 {
    type Output = Sha1Digest;
    const BLOCK_LEN: usize = BLOCK_LEN;
    const OUTPUT_LEN: usize = OUTPUT_LEN;
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

    #[test]
    fn length_rejection_is_atomic() {
        let mut hash = Sha1::new();
        hash.message_len = MAX_BYTES;
        let original_state = hash.state;
        let original_buffer = hash.buffer;
        let original_buffer_len = hash.buffer_len;

        assert_eq!(hash.update([0]), Err(CryptoError::MessageTooLong));
        assert_eq!(hash.state, original_state);
        assert_eq!(hash.buffer, original_buffer);
        assert_eq!(hash.buffer_len, original_buffer_len);
        assert_eq!(hash.message_len, MAX_BYTES);
    }
}
