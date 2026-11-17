//! Secret-bearing values with explicit exposure and destruction.
//!
//! Cryptographic review benefits from seeing exactly where secret bytes become ordinary borrows.
//! [`Secret`] therefore avoids implicit dereferencing and formatting, and clears its owned value
//! when dropped. It cannot clear copies that existed before construction or values explicitly
//! transferred out afterward.
//!
//! # Examples
//!
//! ```
//! use crypto::SecretBytes;
//!
//! let secret = SecretBytes::new(*b"secret!!");
//! assert_eq!(format!("{secret:?}"), "Secret([REDACTED])");
//! assert_eq!(secret.expose_secret(), b"secret!!");
//! ```
//!
//! `into_inner` transfers responsibility to the caller:
//!
//! ```
//! use crypto::SecretBytes;
//!
//! let secret = SecretBytes::new([0x42; 16]);
//! let mut bytes = secret.into_inner();
//! // Use the bytes only at an explicit algorithm boundary, then clear caller-owned storage.
//! bytes.fill(0);
//! assert_eq!(bytes, [0_u8; 16]);
//! ```
//!
//! Mutation is equally explicit:
//!
//! ```
//! use crypto::SecretVec;
//!
//! let mut secret = SecretVec::new(vec![1, 2, 3]);
//! secret.expose_secret_mut()[1] = 9;
//! assert_eq!(secret.expose_secret(), &[1, 9, 3]);
//! ```
//!
//! # Limits of zeroization
//!
//! Zeroization is a lifetime hygiene measure, not proof that a compiler, operating system,
//! allocator, swap device, crash dump, or hardware never copied the data. It complements rather
//! than replaces platform-specific secret-memory review.

use alloc::vec::Vec;
use core::fmt;
use zeroize::Zeroize;

/// A value whose contents are redacted from formatting and zeroized when dropped.
///
/// `Secret` deliberately does not implement [`Clone`], [`Copy`], [`AsRef`], or
/// [`core::ops::Deref`]. Code
/// must call [`expose_secret`](Self::expose_secret) at the point where secret bytes are actually
/// required, making secret propagation visible during review.
///
/// See the [`secret` module](crate::secret) for ownership-transfer examples and limitations.
pub struct Secret<T: Zeroize> {
    value: T,
}

impl<T: Zeroize> Secret<T> {
    /// Wrap a value as secret material.
    pub fn new(value: T) -> Self {
        Self { value }
    }

    /// Borrow the secret value explicitly.
    #[must_use]
    pub fn expose_secret(&self) -> &T {
        &self.value
    }

    /// Mutably borrow the secret value explicitly.
    pub fn expose_secret_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Consume the wrapper and return the value without leaving a second copy in the wrapper.
    ///
    /// The caller becomes responsible for the returned value's lifetime and destruction.
    pub fn into_inner(mut self) -> T
    where
        T: Default,
    {
        core::mem::take(&mut self.value)
    }
}

impl<T: Zeroize + AsRef<[u8]>> Secret<T> {
    /// Whether the secret bytes equal `candidate`, decided without a value-dependent early
    /// exit.
    ///
    /// Use this where a protocol compares a secret it computed with a value an untrusted peer
    /// supplied and no algorithm-specific verifier exists, such as a truncated PRF output.
    /// `==` on slices may stop at the first differing byte, which tells the peer how much of
    /// its guess was right. Every byte of the secret is folded into one difference word, a
    /// length mismatch included, and that word is tested once. The running time depends on
    /// the secret's length, which is treated as public, and not on its contents or on where
    /// the two values differ.
    ///
    /// This is the same OR-fold as the HMAC tag comparison. Like every row of
    /// `SIDE-CHANNELS.md`, it is a source-level property, not a claim about compiled code.
    ///
    /// # Examples
    ///
    /// ```
    /// use crypto::SecretBytes;
    ///
    /// let expected = SecretBytes::new(*b"check value!");
    /// assert!(expected.matches(b"check value!"));
    /// assert!(!expected.matches(b"check value?"));
    /// assert!(!expected.matches(b"check"));
    /// ```
    #[must_use]
    pub fn matches(&self, candidate: impl AsRef<[u8]>) -> bool {
        let secret = self.value.as_ref();
        let candidate = candidate.as_ref();
        let mut difference = secret.len() ^ candidate.len();

        for (index, secret_byte) in secret.iter().copied().enumerate() {
            let candidate_byte = candidate.get(index).copied().unwrap_or(0);
            difference |= usize::from(secret_byte ^ candidate_byte);
        }

        difference == 0
    }
}

impl<T: Zeroize> From<T> for Secret<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: Zeroize> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}

impl<T: Zeroize> Drop for Secret<T> {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

/// Fixed-length secret bytes whose size is part of the type.
///
/// See the [`secret` module](crate::secret) for explicit exposure and ownership examples.
pub type SecretBytes<const N: usize> = Secret<[u8; N]>;

/// Variable-length secret bytes stored in an owned allocation.
///
/// See the [`secret` module](crate::secret) for a mutation example.
pub type SecretVec = Secret<Vec<u8>>;

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::{format, vec};

    #[test]
    fn debug_never_reveals_secret_bytes() {
        let secret = SecretBytes::new(*b"secret!!");
        let rendered = format!("{secret:?}");

        assert_eq!(rendered, "Secret([REDACTED])");
        assert!(!rendered.contains("secret"));
    }

    #[test]
    fn exposure_is_explicit_and_consumption_is_single_owner() {
        let mut secret = SecretVec::new(vec![1, 2, 3]);
        secret.expose_secret_mut()[1] = 9;
        assert_eq!(secret.expose_secret(), &[1, 9, 3]);
        assert_eq!(secret.into_inner(), vec![1, 9, 3]);
    }
    /// Regression evidence; no publication defines this operation.
    #[test]
    fn matches_accepts_only_the_exact_bytes() {
        let secret = SecretBytes::new([0xa5_u8; 12]);
        assert!(secret.matches([0xa5_u8; 12]));

        for bit in 0..12 * 8 {
            let mut candidate = [0xa5_u8; 12];
            candidate[bit / 8] ^= 1 << (bit % 8);
            assert!(!secret.matches(candidate), "bit {bit}");
        }
    }

    /// Regression evidence: a shorter or longer candidate never matches, including one that
    /// agrees on every byte they share and one padded with the zero the fold substitutes.
    #[test]
    fn matches_rejects_every_other_length() {
        let secret = SecretVec::new(vec![7, 0, 0]);
        assert!(secret.matches([7, 0, 0]));
        assert!(!secret.matches([7, 0]));
        assert!(!secret.matches([7]));
        assert!(!secret.matches([7, 0, 0, 0]));
        assert!(!secret.matches([]));
        assert!(SecretVec::new(vec![]).matches([]));
        assert!(!SecretVec::new(vec![]).matches([0]));
    }
}
