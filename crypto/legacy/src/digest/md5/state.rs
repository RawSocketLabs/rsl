//! MD5 streaming, RFC padding, and little-endian output.

use core::fmt;
use zeroize::Zeroize;

use super::compression::{BLOCK_LEN, INITIAL_STATE, compress};
use rsl_crypto::{Result, digest::Digest};

const OUTPUT_LEN: usize = 16;
const LENGTH_START: usize = 56;

/// A historical 128-bit MD5 digest.
#[derive(Clone, Eq, Hash, PartialEq)]
#[must_use = "an MD5 digest has no effect unless its bytes are examined"]
pub struct Md5Digest([u8; OUTPUT_LEN]);

impl Md5Digest {
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
impl AsRef<[u8]> for Md5Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl fmt::Debug for Md5Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Md5Digest(")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

/// Incremental MD5 state for explicit historical use.
#[derive(Clone)]
pub struct Md5 {
    state: [u32; 4],
    buffer: [u8; BLOCK_LEN],
    buffer_len: usize,
    message_len: u64,
}

impl Md5 {
    /// Construct RFC 1321 §3.3's initial state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: INITIAL_STATE,
            buffer: [0; BLOCK_LEN],
            buffer_len: 0,
            message_len: 0,
        }
    }

    /// Add bytes. RFC 1321 retains the message bit length modulo `2^64`.
    ///
    /// # Errors
    ///
    /// This in-memory implementation cannot currently fail. The result preserves the common
    /// digest interface so callers can switch between teaching implementations without changing
    /// their streaming control flow.
    pub fn update(&mut self, input: impl AsRef<[u8]>) -> Result<()> {
        self.update_bytes(input.as_ref());
        Ok(())
    }

    /// Apply RFC 1321 §3.1–§3.2 padding and return the digest.
    pub fn finalize(mut self) -> Md5Digest {
        let bit_length = self.message_len.wrapping_mul(8);
        let mut first = [0_u8; BLOCK_LEN];
        first[..self.buffer_len].copy_from_slice(&self.buffer[..self.buffer_len]);
        first[self.buffer_len] = 0x80;
        if self.buffer_len < LENGTH_START {
            first[LENGTH_START..].copy_from_slice(&bit_length.to_le_bytes());
            self.compress(&first);
        } else {
            self.compress(&first);
            let mut second = [0_u8; BLOCK_LEN];
            second[LENGTH_START..].copy_from_slice(&bit_length.to_le_bytes());
            self.compress(&second);
        }
        let mut output = [0_u8; OUTPUT_LEN];
        for (index, word) in self.state.iter().enumerate() {
            output[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        Md5Digest(output)
    }

    /// Digest one complete byte string with historical MD5.
    ///
    /// # Errors
    ///
    /// This byte-oriented implementation currently has no fallible input for an in-memory slice;
    /// the result remains fallible to satisfy the shared digest contract.
    pub fn digest(input: impl AsRef<[u8]>) -> Result<Md5Digest> {
        let mut state = Self::new();
        state.update(input)?;
        Ok(state.finalize())
    }

    fn update_bytes(&mut self, input: &[u8]) {
        let input_len = u64::try_from(input.len()).expect("an in-memory slice length fits u64");
        self.message_len = self.message_len.wrapping_add(input_len);
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
                return;
            }
        }
        while remaining.len() >= BLOCK_LEN {
            let (block, rest) = remaining.split_at(BLOCK_LEN);
            self.compress(block.try_into().expect("split selects an MD5 block"));
            remaining = rest;
        }
        self.buffer[..remaining.len()].copy_from_slice(remaining);
        self.buffer_len = remaining.len();
    }

    fn compress(&mut self, block: &[u8; BLOCK_LEN]) {
        self.state = compress(self.state, block);
    }
}

impl Default for Md5 {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for Md5 {
    fn drop(&mut self) {
        self.state.zeroize();
        self.buffer.zeroize();
        self.buffer_len.zeroize();
        self.message_len.zeroize();
    }
}
impl Digest for Md5 {
    type Output = Md5Digest;
    const BLOCK_LEN: usize = BLOCK_LEN;
    const OUTPUT_LEN: usize = OUTPUT_LEN;
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &[u8]) -> Result<()> {
        self.update_bytes(input);
        Ok(())
    }
    fn finalize(self) -> Self::Output {
        self.finalize()
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn byte_count_wraps_as_rfc_1321_requires() {
        let mut hash = Md5::new();
        hash.message_len = u64::MAX;
        hash.update([0]).unwrap();
        assert_eq!(hash.message_len, 0);
    }
}
