//! Validated RC4 key ownership and streaming PRGA state.

use alloc::vec::Vec;
use core::fmt;
use zeroize::Zeroize;

use super::key_schedule::{STATE_LEN, schedule};
use crate::{CryptoError, Result, SecretVec};
use rsl_crypto::cipher::StreamCipher as StreamCipherContract;

/// Smallest conventional RC4 key accepted by the algorithm, in bytes.
pub const MIN_KEY_LEN: usize = 1;
/// Largest conventional RC4 key accepted by the algorithm, in bytes.
pub const MAX_KEY_LEN: usize = 256;

/// A validated, owned RC4 key of 1 through 256 bytes.
///
/// The type is deliberately non-`Clone`, redacts formatting, and zeroizes its allocation on drop.
/// It owns the bytes so an [`Rc4`] constructor can consume the only library-managed key owner.
pub struct Rc4Key {
    bytes: SecretVec,
}

impl Rc4Key {
    /// Copy and validate one conventional RC4 key.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidKey`] when `bytes` is empty or longer than 256 bytes.
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self> {
        Self::try_from_vec(bytes.to_vec())
    }

    /// Validate and take ownership of one conventional RC4 key allocation.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidKey`] when `bytes` is empty or longer than 256 bytes.
    pub fn try_from_vec(bytes: Vec<u8>) -> Result<Self> {
        if !(MIN_KEY_LEN..=MAX_KEY_LEN).contains(&bytes.len()) {
            return Err(CryptoError::InvalidKey);
        }
        Ok(Self {
            bytes: SecretVec::new(bytes),
        })
    }

    /// Return the public byte length of this key without exposing its contents.
    #[must_use]
    pub fn key_len(&self) -> usize {
        self.bytes.expose_secret().len()
    }
}

impl fmt::Debug for Rc4Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Rc4Key")
            .field("length", &self.key_len())
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

/// One stateful RC4 keystream position.
///
/// An instance is single-direction state. A sender and receiver need independent instances even
/// when they use identical key bytes. Cloning is intentionally unavailable because it would
/// silently duplicate both secret state and a keystream position.
pub struct Rc4 {
    state: [u8; STATE_LEN],
    i: u8,
    j: u8,
    position: u64,
}

impl Rc4 {
    /// Consume a validated key and run RC4's 256-step KSA.
    #[must_use]
    pub fn new(key: Rc4Key) -> Self {
        let Rc4Key { bytes } = key;
        let state = schedule(bytes.expose_secret());
        Self {
            state,
            i: 0,
            j: 0,
            position: 0,
        }
    }

    /// Return the number of keystream bytes already consumed.
    #[must_use]
    pub const fn position(&self) -> u64 {
        self.position
    }

    /// Consume and conceal `count` keystream bytes.
    ///
    /// This is a raw positioning operation, not a security recommendation. Historical profiles
    /// such as RFC 4345 decide whether and how many bytes must be discarded.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::CounterExhausted`] without changing state if the public position
    /// would exceed `u64::MAX`.
    pub fn discard(&mut self, count: usize) -> Result<()> {
        let next_position = self.next_position(count)?;
        for _ in 0..count {
            let _discarded = self.next_keystream_byte();
        }
        self.position = next_position;
        Ok(())
    }

    /// XOR the next keystream bytes into `buffer` in place.
    ///
    /// Calling this operation a second time with a fresh, identically keyed [`Rc4`] state reverses
    /// the transformation. Reusing a consumed state does not.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::CounterExhausted`] without changing either the cipher or `buffer`
    /// if the public position would exceed `u64::MAX`.
    pub fn apply_keystream(&mut self, buffer: &mut [u8]) -> Result<()> {
        let next_position = self.next_position(buffer.len())?;
        for byte in buffer {
            *byte ^= self.next_keystream_byte();
        }
        self.position = next_position;
        Ok(())
    }

    fn next_position(&self, count: usize) -> Result<u64> {
        let count = u64::try_from(count).map_err(|_| CryptoError::CounterExhausted)?;
        self.position
            .checked_add(count)
            .ok_or(CryptoError::CounterExhausted)
    }

    fn next_keystream_byte(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.state[usize::from(self.i)]);
        self.state.swap(usize::from(self.i), usize::from(self.j));
        let output_index =
            self.state[usize::from(self.i)].wrapping_add(self.state[usize::from(self.j)]);
        self.state[usize::from(output_index)]
    }
}

impl StreamCipherContract for Rc4 {
    fn apply_keystream(&mut self, buffer: &mut [u8]) -> Result<()> {
        Self::apply_keystream(self, buffer)
    }
}

impl fmt::Debug for Rc4 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Rc4")
            .field("state", &"[REDACTED]")
            .field("i", &"[REDACTED]")
            .field("j", &"[REDACTED]")
            .field("position", &self.position)
            .finish()
    }
}

impl Drop for Rc4 {
    fn drop(&mut self) {
        self.state.zeroize();
        self.i.zeroize();
        self.j.zeroize();
        self.position.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::{format, vec};

    #[test]
    fn key_and_state_debug_output_are_redacted() {
        let key = Rc4Key::try_from_slice(b"secret").unwrap();
        assert_eq!(
            format!("{key:?}"),
            "Rc4Key { length: 6, bytes: \"[REDACTED]\" }"
        );
        let cipher = Rc4::new(key);
        assert_eq!(
            format!("{cipher:?}"),
            "Rc4 { state: \"[REDACTED]\", i: \"[REDACTED]\", j: \"[REDACTED]\", position: 0 }"
        );
    }

    #[test]
    fn invalid_key_lengths_are_rejected() {
        assert!(matches!(
            Rc4Key::try_from_slice(&[]),
            Err(CryptoError::InvalidKey)
        ));
        assert!(matches!(
            Rc4Key::try_from_vec(vec![0; MAX_KEY_LEN + 1]),
            Err(CryptoError::InvalidKey)
        ));
        assert_eq!(
            Rc4Key::try_from_vec(vec![0; MAX_KEY_LEN])
                .unwrap()
                .key_len(),
            MAX_KEY_LEN
        );
    }

    #[test]
    fn position_exhaustion_is_atomic() {
        let mut cipher = Rc4::new(Rc4Key::try_from_slice(b"key").unwrap());
        cipher.position = u64::MAX;
        let state = cipher.state;
        let mut byte = [0x5a];
        assert_eq!(
            cipher.apply_keystream(&mut byte),
            Err(CryptoError::CounterExhausted)
        );
        assert_eq!(cipher.state, state);
        assert_eq!(byte, [0x5a]);
    }
}
