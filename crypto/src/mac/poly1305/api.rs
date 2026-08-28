//! Typed Poly1305 one-time key, tag, and incremental authenticator boundary.
//!
//! ## Standards ownership
//!
//! RFC 8439 §2.5 defines Poly1305 over a 32-byte one-time key and any message. The key **must
//! not** be reused for a second message: two tags under one key reveal `r` and forge
//! arbitrarily. The AEAD in §2.8 derives a fresh key per nonce for exactly this reason.

use core::fmt;
use zeroize::Zeroize;

use super::{
    key::{KEY_BYTES, OneTimeKey},
    state::{Accumulator, BLOCK_BYTES},
};
use crate::{CryptoError, Result, SecretBytes, mac::Mac};

/// One owned 32-byte Poly1305 one-time key.
///
/// Non-`Clone`, redacted, and zeroized on drop. Consumed by [`Poly1305::new`].
pub struct Poly1305Key {
    bytes: SecretBytes<KEY_BYTES>,
}

impl Poly1305Key {
    /// Size of a Poly1305 key in bytes.
    pub const LEN: usize = KEY_BYTES;

    /// Take ownership of a one-time key.
    #[must_use]
    pub fn new(bytes: [u8; KEY_BYTES]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }
}

impl fmt::Debug for Poly1305Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Poly1305Key([REDACTED])")
    }
}

/// A 16-byte Poly1305 tag.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Poly1305Tag([u8; BLOCK_BYTES]);

impl Poly1305Tag {
    /// Size of a Poly1305 tag in bytes.
    pub const LEN: usize = BLOCK_BYTES;

    /// Take ownership of tag bytes.
    #[must_use]
    pub const fn new(bytes: [u8; BLOCK_BYTES]) -> Self {
        Self(bytes)
    }

    /// Borrow the tag bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; BLOCK_BYTES] {
        &self.0
    }

    /// Consume the tag into its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; BLOCK_BYTES] {
        self.0
    }
}

impl AsRef<[u8]> for Poly1305Tag {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for Poly1305Tag {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; BLOCK_BYTES]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "Poly1305 tag",
                expected: BLOCK_BYTES,
                actual,
            })?;
        Ok(Self(exact))
    }
}

/// An incremental RFC 8439 §2.5 Poly1305 authenticator.
///
/// Input may arrive in any fragmentation; the state buffers a partial 16-byte block. See the
/// [`poly1305` teaching page](crate::mac::poly1305) for the published example.
pub struct Poly1305 {
    key: OneTimeKey,
    accumulator: Accumulator,
    pending: [u8; BLOCK_BYTES],
    pending_len: usize,
}

impl Poly1305 {
    /// Consume a one-time key and begin authenticating.
    #[must_use]
    pub fn new(key: Poly1305Key) -> Self {
        let mut bytes = key.bytes.into_inner();
        let one_time = OneTimeKey::new(&bytes);
        bytes.zeroize();
        Self {
            key: one_time,
            accumulator: Accumulator::new(),
            pending: [0; BLOCK_BYTES],
            pending_len: 0,
        }
    }

    /// Incorporate more message bytes.
    pub fn update(&mut self, input: impl AsRef<[u8]>) {
        let mut input = input.as_ref();
        if self.pending_len > 0 {
            let take = (BLOCK_BYTES - self.pending_len).min(input.len());
            self.pending[self.pending_len..self.pending_len + take].copy_from_slice(&input[..take]);
            self.pending_len += take;
            input = &input[take..];
            if self.pending_len < BLOCK_BYTES {
                return;
            }
            self.accumulator.absorb(&self.key, &self.pending);
            self.pending_len = 0;
        }
        let mut chunks = input.chunks_exact(BLOCK_BYTES);
        for block in &mut chunks {
            self.accumulator.absorb(&self.key, block);
        }
        let remainder = chunks.remainder();
        self.pending[..remainder.len()].copy_from_slice(remainder);
        self.pending_len = remainder.len();
    }

    /// Absorb any partial final block and produce the tag.
    #[must_use]
    pub fn finalize(mut self) -> Poly1305Tag {
        if self.pending_len > 0 {
            let pending_len = self.pending_len;
            self.accumulator
                .absorb(&self.key, &self.pending[..pending_len]);
            self.pending_len = 0;
        }
        let accumulator = core::mem::replace(&mut self.accumulator, Accumulator::new());
        Poly1305Tag(accumulator.finalize(&self.key))
    }

    /// One-shot authentication of a complete message.
    #[must_use]
    pub fn authenticate(key: Poly1305Key, message: impl AsRef<[u8]>) -> Poly1305Tag {
        let mut mac = Self::new(key);
        mac.update(message);
        mac.finalize()
    }

    /// Compare the computed tag with `expected` without early exit.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::AuthenticationFailed`] when any byte differs or the length is not
    /// sixteen.
    pub fn verify(self, expected: impl AsRef<[u8]>) -> Result<()> {
        let expected = expected.as_ref();
        let computed = self.finalize();
        if expected.len() != BLOCK_BYTES {
            return Err(CryptoError::AuthenticationFailed);
        }
        let difference = computed
            .as_bytes()
            .iter()
            .zip(expected)
            .fold(0_u8, |accumulator, (left, right)| {
                accumulator | (left ^ right)
            });
        if difference == 0 {
            Ok(())
        } else {
            Err(CryptoError::AuthenticationFailed)
        }
    }
}

impl Mac for Poly1305 {
    type Tag = Poly1305Tag;

    fn new(key: &[u8]) -> Result<Self> {
        let actual = key.len();
        let exact = <[u8; KEY_BYTES]>::try_from(key).map_err(|_| CryptoError::InvalidLength {
            name: "Poly1305 key",
            expected: KEY_BYTES,
            actual,
        })?;
        Ok(Self::new(Poly1305Key::new(exact)))
    }

    fn update(&mut self, input: &[u8]) -> Result<()> {
        Self::update(self, input);
        Ok(())
    }

    fn finalize(self) -> Self::Tag {
        Self::finalize(self)
    }

    fn verify(self, expected: &[u8]) -> Result<()> {
        Self::verify(self, expected)
    }
}

impl fmt::Debug for Poly1305 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Poly1305([REDACTED])")
    }
}

impl Drop for Poly1305 {
    fn drop(&mut self) {
        self.pending.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn example_key() -> Poly1305Key {
        Poly1305Key::new([
            0x85, 0xd6, 0xbe, 0x78, 0x57, 0x55, 0x6d, 0x33, 0x7f, 0x44, 0x52, 0xfe, 0x42, 0xd5,
            0x06, 0xa8, 0x01, 0x03, 0x80, 0x8a, 0xfb, 0x0d, 0xb2, 0xfd, 0x4a, 0xbf, 0xf6, 0xaf,
            0x41, 0x49, 0xf5, 0x1b,
        ])
    }

    #[test]
    fn fragmentation_does_not_change_the_tag() {
        let message = b"Cryptographic Forum Research Group";
        let expected = Poly1305::authenticate(example_key(), message);
        for split in [0, 1, 15, 16, 17, 33, 34] {
            let mut mac = Poly1305::new(example_key());
            mac.update(&message[..split]);
            mac.update(&message[split..]);
            assert_eq!(mac.finalize(), expected, "split {split}");
        }
        let mut byte_at_a_time = Poly1305::new(example_key());
        for byte in message {
            byte_at_a_time.update([*byte]);
        }
        assert_eq!(byte_at_a_time.finalize(), expected);
    }

    #[test]
    fn verification_is_uniform_and_length_checked() {
        let expected = Poly1305::authenticate(example_key(), b"m");
        assert!(
            Poly1305::new(example_key())
                .update_then(b"m")
                .verify(expected.as_bytes())
                .is_ok()
        );
        let mut wrong = expected.into_bytes();
        wrong[15] ^= 1;
        assert_eq!(
            Poly1305::new(example_key()).update_then(b"m").verify(wrong),
            Err(CryptoError::AuthenticationFailed)
        );
        assert_eq!(
            Poly1305::new(example_key())
                .update_then(b"m")
                .verify(&wrong[..15]),
            Err(CryptoError::AuthenticationFailed)
        );
    }

    impl Poly1305 {
        fn update_then(mut self, input: &[u8]) -> Self {
            self.update(input);
            self
        }
    }
}
