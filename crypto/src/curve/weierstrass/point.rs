//! Points on an `a = -3` curve: complete projective addition, fixed-structure multiplication,
//! and SEC 1 uncompressed encoding.
//!
//! ## Standards ownership
//!
//! SP 800-186 publishes each generator `G` and requires the group law of `y^2 = x^3 - 3x + b`.
//! SEC 1 §2.3.3 and §2.3.4 define the uncompressed encoding `04 || x || y` and its decoding,
//! including the curve-equation check. The addition formula is Algorithm 4 of Renes, Costello,
//! and Batina, *Complete addition formulas for prime order elliptic curves* (EUROCRYPT 2016;
//! IACR ePrint 2015/1060), specialized to `a = -3`. It is *complete*: one sequence of field
//! operations is correct for every pair of inputs, including doubling, inverse pairs, and the
//! point at infinity `(0 : 1 : 0)`.
//!
//! The 43 steps keep the paper's temporaries `t0..t4` and its exact order so a reviewer can
//! compare line by line. Doubling calls the same formula with both operands equal.

use super::{Curve, field::FieldElement};

/// SEC 1 §2.3.3 prefix octet for an uncompressed point.
pub(crate) const UNCOMPRESSED_PREFIX: u8 = 0x04;

/// A point in homogeneous projective coordinates `(X : Y : Z)` with `x = X/Z`, `y = Y/Z`.
pub(crate) struct ProjectivePoint<C: Curve<N>, const N: usize> {
    x: FieldElement<C, N>,
    y: FieldElement<C, N>,
    z: FieldElement<C, N>,
}

impl<C: Curve<N>, const N: usize> ProjectivePoint<C, N> {
    /// The point at infinity `O = (0 : 1 : 0)`.
    pub(crate) const fn identity() -> Self {
        Self {
            x: FieldElement::ZERO,
            y: FieldElement::ONE,
            z: FieldElement::ZERO,
        }
    }

    /// The fixed generator `G`.
    pub(crate) const fn generator() -> Self {
        Self {
            x: FieldElement::from_limbs(C::GENERATOR_X),
            y: FieldElement::from_limbs(C::GENERATOR_Y),
            z: FieldElement::ONE,
        }
    }

    /// Renes–Costello–Batina Algorithm 4 (`a = -3`), steps 1–43 in printed order.
    #[allow(clippy::many_single_char_names)] // `t0..t4`, `X3`, `Y3`, `Z3` are the paper's names.
    pub(crate) fn add(&self, other: &Self) -> Self {
        let b = FieldElement::<C, N>::from_limbs(C::B);
        let (x1, y1, z1) = (&self.x, &self.y, &self.z);
        let (x2, y2, z2) = (&other.x, &other.y, &other.z);

        let mut t0 = x1.multiply(x2); // 1
        let mut t1 = y1.multiply(y2); // 2
        let mut t2 = z1.multiply(z2); // 3
        let mut t3 = x1.add(y1); // 4
        let mut t4 = x2.add(y2); // 5
        t3 = t3.multiply(&t4); // 6
        t4 = t0.add(&t1); // 7
        t3 = t3.subtract(&t4); // 8
        t4 = y1.add(z1); // 9
        let mut x3 = y2.add(z2); // 10
        t4 = t4.multiply(&x3); // 11
        x3 = t1.add(&t2); // 12
        t4 = t4.subtract(&x3); // 13
        x3 = x1.add(z1); // 14
        let mut y3 = x2.add(z2); // 15
        x3 = x3.multiply(&y3); // 16
        y3 = t0.add(&t2); // 17
        y3 = x3.subtract(&y3); // 18
        let mut z3 = b.multiply(&t2); // 19
        x3 = y3.subtract(&z3); // 20
        z3 = x3.add(&x3); // 21
        x3 = x3.add(&z3); // 22
        z3 = t1.subtract(&x3); // 23
        x3 = t1.add(&x3); // 24
        y3 = b.multiply(&y3); // 25
        t1 = t2.add(&t2); // 26
        t2 = t1.add(&t2); // 27
        y3 = y3.subtract(&t2); // 28
        y3 = y3.subtract(&t0); // 29
        t1 = y3.add(&y3); // 30
        y3 = t1.add(&y3); // 31
        t1 = t0.add(&t0); // 32
        t0 = t1.add(&t0); // 33
        t0 = t0.subtract(&t2); // 34
        t1 = t4.multiply(&y3); // 35
        t2 = t0.multiply(&y3); // 36
        y3 = x3.multiply(&z3); // 37
        y3 = y3.add(&t2); // 38
        x3 = t3.multiply(&x3); // 39
        x3 = x3.subtract(&t1); // 40
        z3 = t4.multiply(&z3); // 41
        t1 = t3.multiply(&t0); // 42
        z3 = z3.add(&t1); // 43

        Self {
            x: x3,
            y: y3,
            z: z3,
        }
    }

    /// Doubling through the complete addition law with equal operands.
    pub(crate) fn double(&self) -> Self {
        self.add(self)
    }

    /// Fixed-structure double-and-add over all `64·N` bits of a big-endian scalar encoding.
    ///
    /// Bits are consumed least-significant first. Every iteration performs one addition, one
    /// masked selection, and one doubling regardless of the bit's value. `scalar_bytes` must be
    /// exactly `8·N` bytes.
    pub(crate) fn multiply(&self, scalar_bytes: &[u8]) -> Self {
        debug_assert_eq!(scalar_bytes.len(), 8 * N);
        let mut result = Self::identity();
        let mut addend = Self::conditional_select(self, self, 0);

        for bit_index in 0..64 * N {
            let sum = result.add(&addend);
            let byte = scalar_bytes[scalar_bytes.len() - 1 - bit_index / 8];
            let bit = u64::from((byte >> (bit_index % 8)) & 1);
            result = Self::conditional_select(&result, &sum, bit);
            addend = addend.double();
        }
        result
    }

    /// Whether `Z = 0`, which identifies the point at infinity in every representation.
    pub(crate) fn is_identity(&self) -> bool {
        self.z.is_zero()
    }

    /// Recover affine `(X/Z, Y/Z)`; `None` for the point at infinity.
    pub(crate) fn to_affine(&self) -> Option<AffinePoint<C, N>> {
        if self.is_identity() {
            return None;
        }
        let inverse_z = self.z.invert();
        Some(AffinePoint {
            x: self.x.multiply(&inverse_z),
            y: self.y.multiply(&inverse_z),
        })
    }

    fn conditional_select(left: &Self, right: &Self, choice: u64) -> Self {
        Self {
            x: FieldElement::conditional_select(&left.x, &right.x, choice),
            y: FieldElement::conditional_select(&left.y, &right.y, choice),
            z: FieldElement::conditional_select(&left.z, &right.z, choice),
        }
    }
}

/// A validated finite point `(x, y)` on the curve.
pub(crate) struct AffinePoint<C: Curve<N>, const N: usize> {
    x: FieldElement<C, N>,
    y: FieldElement<C, N>,
}

impl<C: Curve<N>, const N: usize> AffinePoint<C, N> {
    /// SEC 1 §2.3.4 decoding of `04 || x || y` with the §3.2.2.1 curve-equation check.
    ///
    /// Rejects a wrong length, a prefix other than `0x04`, a coordinate `>= p`, and any pair
    /// with `y^2 != x^3 - 3x + b`. The encoding cannot represent the point at infinity, so a
    /// decoded point is finite; because `n` is prime and `h = 1`, it also has order exactly `n`.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let element = 8 * N;
        if bytes.len() != 1 + 2 * element || bytes[0] != UNCOMPRESSED_PREFIX {
            return None;
        }
        let x = FieldElement::from_canonical_bytes(&bytes[1..=element])?;
        let y = FieldElement::from_canonical_bytes(&bytes[1 + element..])?;
        let point = Self { x, y };
        if point.satisfies_curve_equation() {
            Some(point)
        } else {
            None
        }
    }

    /// SEC 1 §2.3.3 uncompressed encoding `04 || x || y` into `out` (`1 + 16·N` bytes).
    pub(crate) fn write_bytes(&self, out: &mut [u8]) {
        let element = 8 * N;
        assert_eq!(out.len(), 1 + 2 * element);
        out[0] = UNCOMPRESSED_PREFIX;
        self.x.write_bytes(&mut out[1..=element]);
        self.y.write_bytes(&mut out[1 + element..]);
    }

    /// The affine `x`-coordinate, which ECDH outputs and ECDSA compares.
    pub(crate) const fn x(&self) -> &FieldElement<C, N> {
        &self.x
    }

    /// Lift to `(x : y : 1)`.
    pub(crate) fn to_projective(&self) -> ProjectivePoint<C, N> {
        ProjectivePoint {
            x: FieldElement::conditional_select(&self.x, &self.x, 0),
            y: FieldElement::conditional_select(&self.y, &self.y, 0),
            z: FieldElement::ONE,
        }
    }

    /// `y^2 == x^3 - 3x + b`, written as `y^2 == x · (x^2 - 3) + b`.
    fn satisfies_curve_equation(&self) -> bool {
        let mut three = [0_u64; N];
        three[0] = 3;
        let three = FieldElement::from_limbs(three);
        let left = self.y.square();
        let right = self
            .x
            .multiply(&self.x.square().subtract(&three))
            .add(&FieldElement::from_limbs(C::B));
        left.equals(&right)
    }
}
