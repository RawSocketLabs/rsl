//! Key derivation and expansion.
//!
//! A key-derivation function turns existing secret material into one or more purpose-specific
//! byte strings. It does not create entropy. Protocols encode identities, transcript values, or
//! labels into context information so the same input keying material produces independent keys
//! for different purposes.
//!
//! # Implemented algorithm
//!
//! [`hkdf::sha256`] implements RFC 5869 HKDF-SHA-256 with visibly separate Extract and Expand
//! stages.
//!
//! # Generic expansion
//!
//! ```
//! use rsl_crypto::{Result, kdf::{KeyExpander, hkdf::sha256::extract}};
//!
//! fn derive_record_key<E: KeyExpander>(prk: &E) -> Result<[u8; 16]> {
//!     let mut key = [0_u8; 16];
//!     prk.expand(b"example record key", &mut key)?;
//!     Ok(key)
//! }
//!
//! let prk = extract(Some(b"public salt"), b"secret input keying material")?;
//! let key = derive_record_key(&prk)?;
//! assert_ne!(key, [0_u8; 16]);
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```

use crate::Result;

pub mod hkdf;

/// A pseudorandom key that can expand context information into derived bytes.
///
/// HKDF's extracted pseudorandom key is one concrete implementation of this trait. Keeping
/// extraction separate makes the extract-then-expand structure visible rather than presenting
/// HKDF as an opaque single call. See the [`kdf` module](crate::kdf) for a generic example.
pub trait KeyExpander {
    /// Expand `info` into exactly `output.len()` derived bytes.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CryptoError::OutputTooLong`] when the requested output exceeds the
    /// construction's limit.
    fn expand(&self, info: &[u8], output: &mut [u8]) -> Result<()>;
}
