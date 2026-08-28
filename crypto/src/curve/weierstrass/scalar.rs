//! Residues modulo a curve's group order `n`.
//!
//! ## Standards ownership
//!
//! SP 800-186 publishes each prime order `n`. FIPS 186-5 §6.4.2 requires ECDSA verification to
//! reject `r` or `s` outside `[1, n-1]`, to reduce the hash integer `e` modulo `n` through the
//! products it enters, and to compute `s^-1 mod n`. SP 800-56A Rev. 3 §5.6.1.2 requires an ECC
//! private key `d` to lie in `[1, n-1]`, and §5.6.1.2.2 / FIPS 186-5 A.2.2 generate it by
//! testing candidates. This module owns those range rules and the scalar arithmetic.

use core::marker::PhantomData;
use zeroize::Zeroize;

use super::{Curve, arithmetic};
use crate::{CryptoError, Result, random::RandomSource};

/// A residue modulo `n`, capable of holding a private key or per-signature value.
///
/// It is non-`Clone`, non-formattable, and zeroized on drop.
pub(crate) struct Scalar<C: Curve<N>, const N: usize> {
    limbs: [u64; N],
    curve: PhantomData<C>,
}

impl<C: Curve<N>, const N: usize> Scalar<C, N> {
    const fn from_limbs(limbs: [u64; N]) -> Self {
        Self {
            limbs,
            curve: PhantomData,
        }
    }

    /// Decode an `8·N`-byte big-endian integer in `[0, n-1]`, rejecting `n` and above.
    pub(crate) fn from_canonical_bytes(bytes: &[u8]) -> Option<Self> {
        let limbs = arithmetic::from_be_bytes(bytes)?;
        if arithmetic::is_less_than(&limbs, &C::ORDER.value) {
            Some(Self::from_limbs(limbs))
        } else {
            None
        }
    }

    /// Decode an `8·N`-byte big-endian integer in `[1, n-1]`, the private-key and `r`/`s` range.
    pub(crate) fn from_nonzero_canonical_bytes(bytes: &[u8]) -> Option<Self> {
        let scalar = Self::from_canonical_bytes(bytes)?;
        if scalar.is_zero() { None } else { Some(scalar) }
    }

    /// Reduce an `8·N`-byte big-endian integer modulo `n`.
    ///
    /// Such a value is below `2n`, so one masked subtraction is a complete reduction. This is how
    /// a same-width digest becomes ECDSA's `e` and how an `x`-coordinate becomes `v`.
    pub(crate) fn reduce_bytes(bytes: &[u8]) -> Option<Self> {
        let limbs = arithmetic::from_be_bytes(bytes)?;
        Some(Self::from_limbs(C::ORDER.reduce_once(&limbs)))
    }

    /// Reduce a field-domain value (below `p`, hence below `2n`) modulo `n`.
    pub(crate) fn reduce_limbs(limbs: &[u64; N]) -> Self {
        Self::from_limbs(C::ORDER.reduce_once(limbs))
    }

    /// Encode the canonical residue as an `8·N`-byte big-endian integer into `out`.
    pub(crate) fn write_bytes(&self, out: &mut [u8]) {
        arithmetic::write_be_bytes(&self.limbs, out);
    }

    pub(crate) fn add(&self, right: &Self) -> Self {
        Self::from_limbs(C::ORDER.add(&self.limbs, &right.limbs))
    }

    pub(crate) fn multiply(&self, right: &Self) -> Self {
        Self::from_limbs(C::ORDER.multiply(&self.limbs, &right.limbs))
    }

    /// `s^(n-2) = s^-1` for nonzero `s`.
    pub(crate) fn invert(&self) -> Self {
        Self::from_limbs(C::ORDER.power(&self.limbs, &C::ORDER_INVERSION_EXPONENT))
    }

    pub(crate) fn is_zero(&self) -> bool {
        arithmetic::is_zero(&self.limbs)
    }

    pub(crate) fn equals(&self, right: &Self) -> bool {
        self.limbs == right.limbs
    }
}

impl<C: Curve<N>, const N: usize> Drop for Scalar<C, N> {
    fn drop(&mut self) {
        self.limbs.zeroize();
    }
}

/// Candidate draws permitted before generation reports the entropy source as unusable.
///
/// A candidate exceeds `n - 2` with far below `2^-32` probability for every NIST prime-order
/// curve, so a conforming source never approaches this bound.
const MAX_CANDIDATES: usize = 64;

/// FIPS 186-5 Appendix A.2.2 / SP 800-56A Rev. 3 §5.6.1.2.2 private-scalar generation by
/// testing candidates: draw `8·N` bytes `c`, retry while `c > n - 2`, and write `d = c + 1`.
///
/// # Errors
///
/// Returns the source's error, or [`CryptoError::EntropyUnavailable`] if every permitted
/// candidate is out of range, which indicates a non-uniform source.
pub(crate) fn generate_private_bytes<C: Curve<N>, const N: usize, R: RandomSource>(
    random: &mut R,
    out: &mut [u8],
) -> Result<()> {
    let (n_minus_two, _) = arithmetic::subtract_limbs(&C::ORDER.value, &{
        let mut two = [0_u64; N];
        two[0] = 2;
        two
    });
    for _ in 0..MAX_CANDIDATES {
        random.fill_bytes(out)?;
        let Some(mut limbs) = arithmetic::from_be_bytes::<N>(out) else {
            out.zeroize();
            return Err(CryptoError::InvalidLength {
                name: C::NAME,
                expected: 8 * N,
                actual: out.len(),
            });
        };
        if arithmetic::is_less_than(&n_minus_two, &limbs) {
            limbs.zeroize();
            continue;
        }
        let (mut d, _) = arithmetic::add_limbs(&limbs, &arithmetic::one());
        limbs.zeroize();
        arithmetic::write_be_bytes(&d, out);
        d.zeroize();
        return Ok(());
    }
    out.zeroize();
    Err(CryptoError::EntropyUnavailable)
}
