//! SHAKE256: the extendable-output function with `c = 512` (FIPS 202 §6.2).

use core::fmt;

use super::sponge::Sponge;

/// Rate in bytes: `(1600 - 512) / 8`.
const RATE: usize = 136;
/// §6.2 domain suffix `1111` followed by the first `pad10*1` bit, as one byte.
const SUFFIX: u8 = 0x1f;

/// An incremental SHAKE256 computation with arbitrary-length output.
///
/// Output may be read in several calls; each call continues the squeeze where the previous one
/// stopped, so `squeeze(a); squeeze(b)` equals one squeeze of `a.len() + b.len()` bytes. Ed448
/// uses 114- and 64-byte outputs.
///
/// ```
/// use rsl_crypto::digest::sha3::Shake256;
///
/// let mut xof = Shake256::new();
/// xof.update(b"");
/// let mut output = [0_u8; 8];
/// xof.squeeze(&mut output);
/// assert_eq!(output, [0x46, 0xb9, 0xdd, 0x2b, 0x0b, 0xa8, 0x8d, 0x13]);
/// ```
pub struct Shake256 {
    sponge: Sponge<RATE>,
}

impl Shake256 {
    /// Start a new computation.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sponge: Sponge::new(SUFFIX),
        }
    }

    /// Incorporate more message bytes.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if called after the first [`Self::squeeze`]; FIPS 202 defines no
    /// absorb-after-squeeze behaviour.
    pub fn update(&mut self, input: impl AsRef<[u8]>) {
        self.sponge.absorb(input.as_ref());
    }

    /// Fill `output` with the next bytes of the extendable output.
    pub fn squeeze(&mut self, output: &mut [u8]) {
        self.sponge.squeeze(output);
    }

    /// One-shot `SHAKE256(input, 8 · output.len())`.
    pub fn digest_into(input: impl AsRef<[u8]>, output: &mut [u8]) {
        let mut xof = Self::new();
        xof.update(input);
        xof.squeeze(output);
    }
}

impl Default for Shake256 {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Shake256 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Shake256([STATE REDACTED])")
    }
}
