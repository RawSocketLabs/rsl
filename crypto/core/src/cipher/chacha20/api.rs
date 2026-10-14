//! Typed `ChaCha20` key, nonce, one-shot keystream application, and stateful stream boundary.
//!
//! ## Standards ownership
//!
//! RFC 8439 §2.4 defines the `ChaCha20` encryption function: block `j` of the keystream is the
//! block function evaluated at `counter + j`, and each plaintext block is `XORed` with it. The
//! 32-bit counter must not wrap during one nonce's use; this API returns
//! [`CryptoError::CounterExhausted`] rather than reusing keystream. §4 requires the
//! (key, nonce) pair to be unique per message; that is the caller's protocol obligation.

use alloc::vec::Vec;
use core::fmt;
use zeroize::Zeroize;

use super::block::{BLOCK_BYTES, KEY_BYTES, NONCE_BYTES, State};
use crate::{CryptoError, Result, SecretBytes, cipher::StreamCipher, random::RandomSource};

/// One owned 256-bit `ChaCha20` key.
///
/// The owner is non-`Clone`, redacted, and zeroized on drop. Constructing a [`ChaCha20`]
/// consumes it.
///
/// # Examples
///
/// ```
/// use rsl_crypto::cipher::chacha20::ChaCha20Key;
///
/// let key = ChaCha20Key::new([0x42; 32]);
/// assert_eq!(format!("{key:?}"), "ChaCha20Key([REDACTED])");
/// ```
pub struct ChaCha20Key {
    bytes: SecretBytes<KEY_BYTES>,
}

impl ChaCha20Key {
    /// Size of a `ChaCha20` key in bytes.
    pub const LEN: usize = KEY_BYTES;

    /// Take ownership of exactly 256 key bits.
    #[must_use]
    pub fn new(bytes: [u8; KEY_BYTES]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }

    /// Generate a key with the caller-selected randomness source.
    ///
    /// # Errors
    ///
    /// Returns the source's error and clears the partially filled temporary before returning.
    pub fn generate<R: RandomSource>(random: &mut R) -> Result<Self> {
        let mut bytes = [0_u8; KEY_BYTES];
        if let Err(error) = random.fill_bytes(&mut bytes) {
            bytes.zeroize();
            return Err(error);
        }
        Ok(Self::new(bytes))
    }
}

impl fmt::Debug for ChaCha20Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ChaCha20Key([REDACTED])")
    }
}

/// One 96-bit `ChaCha20` nonce in the RFC 8439 (IETF) layout.
///
/// Nonces are public but must never repeat under one key. The type fixes the size; the consuming
/// protocol enforces uniqueness (a counter, or TLS 1.3's sequence-number XOR construction).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChaCha20Nonce([u8; NONCE_BYTES]);

impl ChaCha20Nonce {
    /// Size of a `ChaCha20` nonce in bytes.
    pub const LEN: usize = NONCE_BYTES;

    /// Take ownership of exactly 96 nonce bits.
    #[must_use]
    pub const fn new(bytes: [u8; NONCE_BYTES]) -> Self {
        Self(bytes)
    }

    /// Borrow the nonce bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; NONCE_BYTES] {
        &self.0
    }

    /// Consume the nonce into its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; NONCE_BYTES] {
        self.0
    }
}

impl AsRef<[u8]> for ChaCha20Nonce {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for ChaCha20Nonce {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; NONCE_BYTES]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "ChaCha20 nonce",
                expected: NONCE_BYTES,
                actual,
            })?;
        Ok(Self(exact))
    }
}

/// RFC 8439 `ChaCha20` keyed for one-shot keystream generation.
///
/// This is a raw stream cipher: it provides confidentiality only. Use
/// [`ChaCha20Poly1305`](crate::aead::chacha20poly1305::ChaCha20Poly1305) for authenticated
/// encryption. See the [`chacha20` teaching page](crate::cipher::chacha20) for the block
/// structure and a published example.
pub struct ChaCha20 {
    key: SecretBytes<KEY_BYTES>,
}

impl ChaCha20 {
    /// Consume a key.
    #[must_use]
    pub fn new(key: ChaCha20Key) -> Self {
        Self { key: key.bytes }
    }

    /// RFC 8439 §2.3: the 64-byte keystream block at one counter value.
    ///
    /// Exposed because §2.6 derives the Poly1305 one-time key from block zero.
    #[must_use]
    pub fn keystream_block(&self, counter: u32, nonce: &ChaCha20Nonce) -> [u8; BLOCK_BYTES] {
        State::new(self.key.expose_secret(), counter, nonce.as_bytes()).keystream_block()
    }

    /// RFC 8439 §2.4: XOR the keystream starting at `initial_counter` into `buffer`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::CounterExhausted`] before touching `buffer` if the blocks needed
    /// would carry the 32-bit counter past `2^32 - 1`.
    pub fn apply_keystream(
        &self,
        initial_counter: u32,
        nonce: &ChaCha20Nonce,
        buffer: &mut [u8],
    ) -> Result<()> {
        let blocks = u32::try_from(buffer.len().div_ceil(BLOCK_BYTES))
            .map_err(|_| CryptoError::CounterExhausted)?;
        if blocks > 0 {
            initial_counter
                .checked_add(blocks - 1)
                .ok_or(CryptoError::CounterExhausted)?;
        }
        let mut counter = initial_counter;
        for chunk in buffer.chunks_mut(BLOCK_BYTES) {
            let mut keystream = self.keystream_block(counter, nonce);
            for (byte, key_byte) in chunk.iter_mut().zip(keystream.iter()) {
                *byte ^= key_byte;
            }
            keystream.zeroize();
            counter = counter.wrapping_add(1);
        }
        Ok(())
    }

    /// RFC 8439 §2.4 as an owned-output operation; encryption and decryption are identical.
    ///
    /// # Errors
    ///
    /// As for [`Self::apply_keystream`].
    pub fn encrypt(
        &self,
        initial_counter: u32,
        nonce: &ChaCha20Nonce,
        data: &[u8],
    ) -> Result<Vec<u8>> {
        let mut output = data.to_vec();
        self.apply_keystream(initial_counter, nonce, &mut output)?;
        Ok(output)
    }

    /// Begin a stateful keystream at `initial_counter` for the [`StreamCipher`] contract.
    #[must_use]
    pub fn stream(self, nonce: ChaCha20Nonce, initial_counter: u32) -> ChaCha20Stream {
        ChaCha20Stream {
            cipher: self,
            nonce,
            next_counter: Some(initial_counter),
            buffered: [0; BLOCK_BYTES],
            buffered_offset: BLOCK_BYTES,
        }
    }
}

impl fmt::Debug for ChaCha20 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ChaCha20([REDACTED KEY])")
    }
}

/// A position-tracking `ChaCha20` keystream for incremental use.
///
/// It buffers at most one 64-byte block so successive calls of any length continue exactly where
/// the previous one stopped. When the counter would wrap, every further call fails with
/// [`CryptoError::CounterExhausted`].
pub struct ChaCha20Stream {
    cipher: ChaCha20,
    nonce: ChaCha20Nonce,
    next_counter: Option<u32>,
    buffered: [u8; BLOCK_BYTES],
    buffered_offset: usize,
}

impl ChaCha20Stream {
    /// XOR the next keystream bytes into `buffer`, continuing from the previous position.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::CounterExhausted`] without modifying `buffer` if the request would
    /// require a block beyond counter `2^32 - 1`.
    pub fn apply_keystream(&mut self, buffer: &mut [u8]) -> Result<()> {
        let available = BLOCK_BYTES - self.buffered_offset;
        if buffer.len() > available {
            let needed_blocks = (buffer.len() - available).div_ceil(BLOCK_BYTES);
            let next = self.next_counter.ok_or(CryptoError::CounterExhausted)?;
            let last_block = u32::try_from(needed_blocks - 1)
                .ok()
                .and_then(|offset| next.checked_add(offset))
                .ok_or(CryptoError::CounterExhausted)?;
            debug_assert!(last_block >= next);
        }
        for byte in buffer.iter_mut() {
            if self.buffered_offset == BLOCK_BYTES {
                let Some(counter) = self.next_counter else {
                    return Err(CryptoError::CounterExhausted);
                };
                self.buffered = self.cipher.keystream_block(counter, &self.nonce);
                self.buffered_offset = 0;
                self.next_counter = counter.checked_add(1);
            }
            *byte ^= self.buffered[self.buffered_offset];
            self.buffered_offset += 1;
        }
        Ok(())
    }
}

impl StreamCipher for ChaCha20Stream {
    fn apply_keystream(&mut self, buffer: &mut [u8]) -> Result<()> {
        Self::apply_keystream(self, buffer)
    }
}

impl fmt::Debug for ChaCha20Stream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChaCha20Stream")
            .field("nonce", &self.nonce)
            .field("next_counter", &self.next_counter)
            .field("keystream", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl Drop for ChaCha20Stream {
    fn drop(&mut self) {
        self.buffered.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn counter_exhaustion_is_reported_before_any_keystream_is_applied() {
        let cipher = ChaCha20::new(ChaCha20Key::new([0x42; 32]));
        let nonce = ChaCha20Nonce::new([0; 12]);
        let mut two_blocks = [0x11_u8; 128];
        assert_eq!(
            cipher.apply_keystream(u32::MAX, &nonce, &mut two_blocks),
            Err(CryptoError::CounterExhausted)
        );
        assert_eq!(two_blocks, [0x11; 128]);
        let mut one_block = [0_u8; 64];
        assert!(
            cipher
                .apply_keystream(u32::MAX, &nonce, &mut one_block)
                .is_ok()
        );
        assert!(cipher.apply_keystream(u32::MAX, &nonce, &mut []).is_ok());
    }

    #[test]
    fn stateful_stream_matches_one_shot_across_arbitrary_splits() {
        let expected = ChaCha20::new(ChaCha20Key::new([0x42; 32]))
            .encrypt(7, &ChaCha20Nonce::new([1; 12]), &[0_u8; 200])
            .unwrap();
        let mut stream =
            ChaCha20::new(ChaCha20Key::new([0x42; 32])).stream(ChaCha20Nonce::new([1; 12]), 7);
        let mut actual = [0_u8; 200];
        let mut position = 0;
        for length in [1, 63, 64, 65, 7] {
            stream
                .apply_keystream(&mut actual[position..position + length])
                .unwrap();
            position += length;
        }
        assert_eq!(&actual[..position], &expected[..position]);

        let mut ending = ChaCha20::new(ChaCha20Key::new([0x42; 32]))
            .stream(ChaCha20Nonce::new([1; 12]), u32::MAX);
        let mut block = [0_u8; 64];
        ending.apply_keystream(&mut block).unwrap();
        assert_eq!(
            ending.apply_keystream(&mut [0_u8; 1]),
            Err(CryptoError::CounterExhausted)
        );
    }
}
