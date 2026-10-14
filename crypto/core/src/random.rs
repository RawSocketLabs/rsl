//! Randomness-source contracts.
//!
//! Cryptographic algorithms need an explicit boundary between deterministic mathematics and an
//! operating-system or hardware entropy source. This crate defines that boundary but does not
//! silently choose a platform RNG. Adapters belong in integration crates where blocking,
//! initialization, failure, and platform policy can be reviewed.
//!
//! # Test-only implementation example
//!
//! The deterministic source below is useful only for tests; it is not random and must never
//! generate real keys or nonces.
//!
//! ```
//! use rsl_crypto::{Result, random::RandomSource};
//!
//! struct DeterministicTestSource(u8);
//!
//! impl RandomSource for DeterministicTestSource {
//!     fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
//!         for byte in output {
//!             *byte = self.0;
//!             self.0 = self.0.wrapping_add(1);
//!         }
//!         Ok(())
//!     }
//! }
//!
//! let mut source = DeterministicTestSource(0);
//! let mut bytes = [0_u8; 4];
//! source.fill_bytes(&mut bytes)?;
//! assert_eq!(bytes, [0, 1, 2, 3]);
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```

use crate::Result;

/// A source capable of filling cryptographic random bytes.
///
/// Operating-system and hardware adapters belong in separate integration modules or crates; the
/// mathematical primitives depend only on this narrow interface.
///
/// Returning `Ok(())` promises that every requested byte was filled. Implementations must not
/// report success after a partial read.
pub trait RandomSource {
    /// Fill every byte of `output` or return an error without claiming partial success.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CryptoError::EntropyUnavailable`] when the source cannot provide all
    /// requested bytes.
    fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()>;
}
