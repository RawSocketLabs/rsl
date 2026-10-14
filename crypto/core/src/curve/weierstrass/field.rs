//! Residues modulo a curve's field prime `p`.
//!
//! ## Standards ownership
//!
//! SP 800-186 publishes each prime; SEC 1 §2.3.5 fixes the field-element-to-octet-string
//! conversion as an `8·N`-byte big-endian integer, and §2.3.6 rejects octet strings whose integer
//! is not below `p`. This module owns those rules and the inversion `z^(p-2)` used to recover
//! affine coordinates.

use core::marker::PhantomData;
use zeroize::Zeroize;

use super::{Curve, arithmetic};

/// A residue modulo `p`, capable of holding a secret-derived coordinate.
///
/// It is non-`Clone`, non-formattable, and zeroized on drop.
pub(crate) struct FieldElement<C: Curve<N>, const N: usize> {
    limbs: [u64; N],
    curve: PhantomData<C>,
}

impl<C: Curve<N>, const N: usize> FieldElement<C, N> {
    pub(crate) const ZERO: Self = Self::from_limbs([0; N]);

    pub(crate) const ONE: Self = Self::from_limbs(arithmetic::one());

    /// Wrap limbs already known to be below `p`, such as published constants.
    pub(crate) const fn from_limbs(limbs: [u64; N]) -> Self {
        Self {
            limbs,
            curve: PhantomData,
        }
    }

    /// SEC 1 §2.3.6: decode an `8·N`-byte big-endian integer, rejecting values `>= p`.
    pub(crate) fn from_canonical_bytes(bytes: &[u8]) -> Option<Self> {
        let limbs = arithmetic::from_be_bytes(bytes)?;
        if arithmetic::is_less_than(&limbs, &C::FIELD.value) {
            Some(Self::from_limbs(limbs))
        } else {
            None
        }
    }

    /// SEC 1 §2.3.5: encode the residue as an `8·N`-byte big-endian integer into `out`.
    pub(crate) fn write_bytes(&self, out: &mut [u8]) {
        arithmetic::write_be_bytes(&self.limbs, out);
    }

    pub(crate) fn add(&self, right: &Self) -> Self {
        Self::from_limbs(C::FIELD.add(&self.limbs, &right.limbs))
    }

    pub(crate) fn subtract(&self, right: &Self) -> Self {
        Self::from_limbs(C::FIELD.subtract(&self.limbs, &right.limbs))
    }

    pub(crate) fn multiply(&self, right: &Self) -> Self {
        Self::from_limbs(C::FIELD.multiply(&self.limbs, &right.limbs))
    }

    pub(crate) fn square(&self) -> Self {
        self.multiply(self)
    }

    /// `z^(p-2) = z^-1` for nonzero `z`; zero maps to zero.
    pub(crate) fn invert(&self) -> Self {
        Self::from_limbs(C::FIELD.power(&self.limbs, &C::FIELD_INVERSION_EXPONENT))
    }

    /// Select `right` when `choice` is one and `left` when it is zero without branching.
    pub(crate) fn conditional_select(left: &Self, right: &Self, choice: u64) -> Self {
        Self::from_limbs(arithmetic::select(&left.limbs, &right.limbs, choice))
    }

    pub(crate) fn equals(&self, right: &Self) -> bool {
        self.limbs == right.limbs
    }

    pub(crate) fn is_zero(&self) -> bool {
        arithmetic::is_zero(&self.limbs)
    }

    /// Borrow the canonical limbs for a scalar-domain reinterpretation of `x`.
    pub(crate) const fn limbs(&self) -> &[u64; N] {
        &self.limbs
    }
}

impl<C: Curve<N>, const N: usize> Drop for FieldElement<C, N> {
    fn drop(&mut self) {
        self.limbs.zeroize();
    }
}
