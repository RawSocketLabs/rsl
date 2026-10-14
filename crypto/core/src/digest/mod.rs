//! Fixed-output cryptographic digests.
//!
//! A digest maps an arbitrary-length byte string to a fixed-length value. The mapping is
//! deterministic and unkeyed: the same bytes always produce the same digest. Digests support
//! integrity checks when the expected digest is already trusted, but they do not authenticate an
//! attacker-controlled message. Use a MAC such as [`crate::mac::hmac::sha256::HmacSha256`] when a
//! secret key must distinguish authorized messages.
//!
//! # Implemented algorithm
//!
//! [`sha2::sha256`] contains the readable SHA-256 implementation and its teaching guide.
//!
//! # Generic incremental use
//!
//! ```
//! use rsl_crypto::{Result, digest::{Digest, sha2::sha256::Sha256}};
//!
//! fn digest_fragments<D: Digest>(fragments: &[&[u8]]) -> Result<D::Output> {
//!     let mut state = D::new();
//!     for fragment in fragments {
//!         state.update(fragment)?;
//!     }
//!     Ok(state.finalize())
//! }
//!
//! let digest = digest_fragments::<Sha256>(&[b"readable ", b"fragments"])?;
//! assert_eq!(digest, Sha256::digest(b"readable fragments")?);
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```

use crate::Result;

pub mod sha2;
pub mod sha3;

/// An incremental fixed-output cryptographic digest.
///
/// Concrete output types should be distinct newtypes rather than interchangeable byte vectors.
/// This makes accidental mixing of, for example, a SHA-256 digest and an HMAC tag a type error.
/// See the [`digest` module](crate::digest) for a generic runnable example.
pub trait Digest: Sized {
    /// The finalized digest value.
    type Output: AsRef<[u8]>;

    /// The compression-function input block length in bytes.
    const BLOCK_LEN: usize;

    /// The finalized output length in bytes.
    const OUTPUT_LEN: usize;

    /// Construct a digest in its algorithm-defined initial state.
    fn new() -> Self;

    /// Incorporate more message bytes.
    ///
    /// # Errors
    ///
    /// Returns an error before modifying the state if the algorithm's message-length limit would
    /// be exceeded.
    fn update(&mut self, input: &[u8]) -> Result<()>;

    /// Apply padding and return the finalized digest.
    ///
    /// Implementations validate fallible message limits during [`update`](Self::update), so a
    /// successfully accumulated state finalizes without another failure path.
    fn finalize(self) -> Self::Output;

    /// Digest one complete byte string.
    ///
    /// # Errors
    ///
    /// Returns an error if `input` exceeds the algorithm's message-length limit.
    fn digest(input: impl AsRef<[u8]>) -> Result<Self::Output> {
        let mut state = Self::new();
        state.update(input.as_ref())?;
        Ok(state.finalize())
    }
}
