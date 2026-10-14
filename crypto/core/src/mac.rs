//! Keyed message authentication codes.
//!
//! A MAC combines secret key material with message bytes and produces a public tag. A verifier
//! holding the same key recomputes and compares that tag. Unlike an unkeyed digest, a MAC lets the
//! verifier reject modifications made by someone who does not know the key.
//!
//! # Implemented algorithm
//!
//! [`hmac::sha256`] implements HMAC-SHA-256 and explains its nested-hash construction.
//!
//! # Generic use
//!
//! ```
//! use rsl_crypto::{Result, mac::{Mac, hmac::sha256::HmacSha256}};
//!
//! fn authenticate_fragments<M: Mac>(key: &[u8], fragments: &[&[u8]]) -> Result<M::Tag> {
//!     let mut state = M::new(key)?;
//!     for fragment in fragments {
//!         state.update(fragment)?;
//!     }
//!     Ok(state.finalize())
//! }
//!
//! let tag = authenticate_fragments::<HmacSha256>(b"teaching key", &[b"part one", b" + two"])?;
//! let mut verifier = HmacSha256::new(b"teaching key")?;
//! verifier.update(b"part one + two")?;
//! verifier.verify(tag.as_bytes())?;
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```

use crate::Result;

pub mod hmac;
pub mod poly1305;

/// An incremental keyed message-authentication code.
///
/// See the [`mac` module](crate::mac) for a generic runnable example.
pub trait Mac: Sized {
    /// The finalized authentication tag.
    type Tag: AsRef<[u8]>;

    /// Construct a MAC context from key bytes.
    ///
    /// Concrete algorithms copy the key into secret-bearing storage before this call returns.
    ///
    /// # Errors
    ///
    /// Returns an algorithm error when the key length or representation is invalid.
    fn new(key: &[u8]) -> Result<Self>;

    /// Incorporate more authenticated message bytes.
    ///
    /// # Errors
    ///
    /// Returns an error before modifying the state if the underlying construction's message
    /// length limit would be exceeded.
    fn update(&mut self, input: &[u8]) -> Result<()>;

    /// Return the finalized authentication tag.
    fn finalize(self) -> Self::Tag;

    /// Verify an expected tag without returning a computed tag to the caller.
    ///
    /// Implementations must compare tags without secret-dependent early exit.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CryptoError::AuthenticationFailed`] when the tag does not verify.
    fn verify(self, expected: &[u8]) -> Result<()>;
}
