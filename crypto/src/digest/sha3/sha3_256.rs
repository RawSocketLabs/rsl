//! SHA3-256: the fixed-output SHA-3 function with `c = 512` (FIPS 202 §6.1).

use core::fmt;

use super::sponge::Sponge;
use crate::{Result, digest::Digest};

/// Rate in bytes: `(1600 - 512) / 8`.
const RATE: usize = 136;
/// Output length in bytes.
const DIGEST_LEN: usize = 32;
/// §6.1 domain suffix `01` followed by the first `pad10*1` bit, as one byte.
const SUFFIX: u8 = 0x06;

/// A finalized 256-bit SHA3-256 digest.
#[derive(Clone, Eq, Hash, PartialEq)]
#[must_use = "a SHA3-256 digest should be compared, stored, or otherwise consumed"]
pub struct Sha3_256Digest([u8; DIGEST_LEN]);

impl Sha3_256Digest {
    /// Serialized digest length.
    pub const LEN: usize = DIGEST_LEN;

    /// Borrow all 32 digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIGEST_LEN] {
        &self.0
    }

    /// Consume the digest into its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; DIGEST_LEN] {
        self.0
    }
}

impl AsRef<[u8]> for Sha3_256Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Sha3_256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Sha3_256Digest(")?;
        for byte in &self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

/// An incremental SHA3-256 computation.
///
/// ```
/// use rsl_crypto::digest::sha3::Sha3_256;
///
/// let digest = Sha3_256::digest("")?;
/// assert_eq!(&digest.as_bytes()[..4], &[0xa7, 0xff, 0xc6, 0xf8]);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub struct Sha3_256 {
    sponge: Sponge<RATE>,
}

impl Sha3_256 {
    /// Start a new computation.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sponge: Sponge::new(SUFFIX),
        }
    }

    /// Incorporate more message bytes. The sponge has no length limit.
    ///
    /// # Errors
    ///
    /// Never fails; the `Result` keeps the signature uniform with the SHA-2 digests.
    pub fn update(&mut self, input: impl AsRef<[u8]>) -> Result<()> {
        self.sponge.absorb(input.as_ref());
        Ok(())
    }

    /// Pad, permute, and squeeze the 32-byte digest.
    pub fn finalize(mut self) -> Sha3_256Digest {
        let mut output = [0_u8; DIGEST_LEN];
        self.sponge.squeeze(&mut output);
        Sha3_256Digest(output)
    }

    /// Digest one complete byte representation.
    ///
    /// # Errors
    ///
    /// Never fails; see [`Self::update`].
    pub fn digest(input: impl AsRef<[u8]>) -> Result<Sha3_256Digest> {
        let mut state = Self::new();
        state.update(input)?;
        Ok(state.finalize())
    }
}

impl Default for Sha3_256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Digest for Sha3_256 {
    type Output = Sha3_256Digest;
    const BLOCK_LEN: usize = RATE;
    const OUTPUT_LEN: usize = DIGEST_LEN;

    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &[u8]) -> Result<()> {
        self.sponge.absorb(input);
        Ok(())
    }
    fn finalize(self) -> Self::Output {
        self.finalize()
    }
}
