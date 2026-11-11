//! Arbitrary-width unsigned integers (`u1`..`u127`) — the substrate for sub-byte
//! bitfield fields. Replaces the `arbitrary-int` dependency.
//!
//! [`UInt<T, N>`] wraps the smallest primitive `T` that can hold `N` bits and
//! enforces the `N`-bit range. The type aliases ([`u1`], [`u5`], …) pick the
//! right backing for you; the native widths (`u8`, `u16`, `u32`, `u64`, `u128`)
//! are just the standard library's and need no wrapper.
//!
//! ```
//! use bnb::u5;
//!
//! let x = u5::new(17);          // panics if > 31
//! assert_eq!(x.value(), 17);
//! assert_eq!(u5::MAX.value(), 31);
//! assert!(u5::try_new(32).is_err());
//! ```
//!
//! # Converting from another integer type
//!
//! Convert with `try_from`/`try_into`, never with an `as` cast followed by a
//! constructor: the cast truncates *before* the range check can see the value.
//! `TryFrom` is implemented for every primitive integer (`u8`..`u128`, `usize`,
//! `i8`..`i128`, `isize`) and is `Ok` exactly when the mathematical value fits in
//! `N` bits; nothing wraps, truncates or reinterprets a sign.
//!
//! ```
//! use bnb::{u12, WidthError};
//!
//! let len: usize = 65_541; // 0x1_0005
//! assert_eq!(u12::try_new(len as u16).map(u12::value), Ok(5)); // wrong: the cast wrapped
//! assert_eq!(
//!     u12::try_from(len),
//!     Err(WidthError::ValueTooLarge { value: 65_541, bits: 12 }),
//! );
//! assert_eq!(u12::try_from(-1), Err(WidthError::Negative { value: -1, bits: 12 }));
//! assert_eq!(u12::try_from(4095_u64).map(u12::value), Ok(4095));
//! ```
//!
//! **An untyped argument is a signed `i32`.** An integer expression whose type nothing
//! else fixes is evaluated as `i32` before it is converted, exactly as in
//! `u64::try_from(..)`; it is not inferred to be the backing type. That is harmless for a
//! small literal (`u12::try_from(5)`), but `!`, `>>`, `/`, `%` and bit 31 mean something
//! else on a signed value: `u12::try_from(!0 >> 4)` is `Err(Negative)`, not `Ok(4095)`,
//! and `u48::try_from(1 << 40)` overflows the `i32`. Give one operand a type whenever
//! the argument is more than a small literal: `u12::try_from(!0_u16 >> 4)`,
//! `u48::try_from(1_u64 << 40)`. A suffix is also what lets a method be called on the
//! error of a literal conversion (`u12::try_from(5000_u32).unwrap_err().to_string()`).

use core::fmt;

use crate::error::WidthError;
use crate::field::Bits;

/// An unsigned integer constrained to `N` bits, backed by primitive `T`.
///
/// Use the aliases ([`u5`], [`u13`], …) rather than naming `T` directly. The
/// value is always in `0..=MAX` (`MAX == 2^N - 1`); construct with
/// [`new`](UInt::new) (panicking) or [`try_new`](UInt::try_new) (checked), and
/// read it back with [`value`](UInt::value).
///
/// # Conversions
///
/// `TryFrom` accepts every primitive integer type, signed or unsigned, and is `Ok`
/// exactly when the mathematical value fits in `N` bits: too large is
/// [`WidthError::ValueTooLarge`], negative is [`WidthError::Negative`]. Use it instead
/// of casting to the backing type first: `u12::try_new(len as u16)` range-checks
/// what the cast left, `u12::try_from(len)` range-checks `len`. Going back, `From`
/// yields the backing primitive (`u16::from(x)` for a [`u12`]).
///
/// # Formatting
///
/// `Display`, `LowerHex`, `UpperHex`, `Octal` and `Binary` print the value as the
/// backing primitive would, honouring the same flags and width; `Debug` adds the
/// width (`u12(2748)`).
///
/// # Examples
///
/// ```
/// use bnb::{u5, u12};
///
/// let a = u5::new(17);              // checked; panics if > 31
/// assert_eq!(a.value(), 17);
/// assert!(u5::try_new(32).is_err()); // out of range -> Err, no panic
/// assert_eq!(u5::from_raw(0xFF).value(), 31); // masks to the low 5 bits
/// assert_eq!(u5::MAX.value(), 31);
/// assert_eq!(u5::MIN.value(), 0);
///
/// let b = u12::try_from(2748_usize)?; // any primitive integer, range-checked
/// assert_eq!(u16::from(b), 0xABC);
/// assert_eq!(format!("{b:#06x}"), "0x0abc");
/// assert!(u12::try_from(-1_i64).is_err());
/// # Ok::<(), bnb::WidthError>(())
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UInt<T, const N: usize> {
    value: T,
}

/// Implements the radix formatting traits for `UInt<$t, N>` by delegating to the
/// backing value, so flags, width and `#` behave exactly as on the primitive.
macro_rules! impl_uint_radix_fmt {
    ($t:ty; $($radix:ident),*) => {
        $(
            impl<const N: usize> fmt::$radix for UInt<$t, N> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    fmt::$radix::fmt(&self.value, f)
                }
            }
        )*
    };
}

/// Implements `TryFrom<S> for UInt<$t, N>` for every primitive integer `S` other than
/// the backing type `$t` itself, which [`impl_uint!`] covers. The caller lists the
/// other unsigned sources; the signed ones are the same for every backing.
///
/// Each impl narrows with the primitive's own checked `TryFrom` and then applies the
/// `N`-bit check, so no value is cast before it has been range-checked.
macro_rules! impl_uint_try_from {
    ($t:ty; $($unsigned:ty),*) => {
        impl_uint_try_from!(@unsigned $t; $($unsigned),*);
        impl_uint_try_from!(@signed $t; i8, i16, i32, i64, i128, isize);
    };
    (@unsigned $t:ty; $($s:ty),*) => {
        $(
            impl<const N: usize> TryFrom<$s> for UInt<$t, N> {
                type Error = WidthError;

                /// Converts `value`, or returns [`WidthError::ValueTooLarge`] if it
                /// does not fit in `N` bits.
                #[inline]
                // Widening to `u128` is lossless for every unsigned primitive; `usize`
                // has no `From` conversion into it.
                #[allow(clippy::cast_lossless)]
                fn try_from(value: $s) -> core::result::Result<Self, WidthError> {
                    <$t>::try_from(value)
                        .map_err(|_| WidthError::ValueTooLarge {
                            value: value as u128,
                            bits: Self::BITS,
                        })
                        .and_then(Self::try_new)
                }
            }
        )*
    };
    (@signed $t:ty; $($s:ty),*) => {
        $(
            impl<const N: usize> TryFrom<$s> for UInt<$t, N> {
                type Error = WidthError;

                /// Converts `value`, or returns [`WidthError::Negative`] if it is
                /// below zero and [`WidthError::ValueTooLarge`] if it does not fit in
                /// `N` bits.
                #[inline]
                // Widening to `i128` is lossless for every signed primitive; `isize`
                // has no `From` conversion into it.
                #[allow(clippy::cast_lossless)]
                fn try_from(value: $s) -> core::result::Result<Self, WidthError> {
                    <$t>::try_from(value)
                        .map_err(|_| rejected_signed(value as i128, Self::BITS))
                        .and_then(Self::try_new)
                }
            }
        )*
    };
}

/// The error for a signed source value its target's backing type cannot hold: negative,
/// or positive and above the backing's range (so above any `bits`-wide range too).
fn rejected_signed(value: i128, bits: u32) -> WidthError {
    if value < 0 {
        WidthError::Negative { value, bits }
    } else {
        WidthError::ValueTooLarge {
            value: value.unsigned_abs(),
            bits,
        }
    }
}

macro_rules! impl_uint {
    ($($t:ty),* $(,)?) => {
        $(
            impl<const N: usize> UInt<$t, N> {
                const VALID_WIDTH: () = assert!(N <= <$t>::BITS as usize, "UInt width exceeds its backing integer");
                /// The number of bits.
                pub const BITS: u32 = <Self as Bits>::BITS;

                /// A mask with the low `N` bits set.
                pub const MASK: $t = {
                    let () = Self::VALID_WIDTH;
                    if N == <$t>::BITS as usize {
                        <$t>::MAX
                    } else {
                        ((1 as $t) << N) - 1
                    }
                };

                /// The largest representable value (`2^N - 1`).
                pub const MAX: Self = Self { value: Self::MASK };

                /// The zero value.
                pub const MIN: Self = Self::from_raw(0);

                /// Creates a value, panicking if it does not fit in `N` bits.
                ///
                /// # Panics
                /// Panics if `value > MAX`.
                #[inline]
                pub const fn new(value: $t) -> Self {
                    assert!(value <= Self::MASK, "value out of range for this width");
                    Self { value }
                }

                /// Creates a value, or [`WidthError::ValueTooLarge`] if it does not
                /// fit in `N` bits.
                ///
                /// # Errors
                /// Returns [`WidthError::ValueTooLarge`] when `value` exceeds the width.
                #[inline]
                pub fn try_new(value: $t) -> core::result::Result<Self, WidthError> {
                    if value <= Self::MASK {
                        Ok(Self { value })
                    } else {
                        Err(WidthError::ValueTooLarge {
                            value: u128::from(value),
                            bits: Self::BITS,
                        })
                    }
                }

                /// Creates a value from the low `N` bits of `value`, discarding
                /// any higher bits (the unchecked, masking constructor).
                #[inline]
                pub const fn from_raw(value: $t) -> Self {
                    Self { value: value & Self::MASK }
                }

                /// The underlying value.
                #[inline]
                pub const fn value(self) -> $t {
                    let () = Self::VALID_WIDTH;
                    self.value
                }

                // Const-dispatch seam for macro-generated accessors: the
                // `Bits::from_bits`/`into_bits` contract as inherent `const fn`s,
                // callable where trait methods cannot be (const trait impls are
                // unstable). The `Bits` impl delegates here, so the two can't drift.
                #[doc(hidden)]
                #[inline]
                #[allow(clippy::cast_possible_truncation)] // Low-bit truncation is the Bits contract.
                pub const fn __bnb_from_bits(raw: u128) -> Self {
                    Self::from_raw(raw as $t)
                }

                #[doc(hidden)]
                #[inline]
                #[allow(clippy::cast_lossless)] // From is not const on the MSRV.
                pub const fn __bnb_into_bits(self) -> u128 {
                    self.value() as u128
                }
            }

            impl<const N: usize> Default for UInt<$t, N> {
                #[inline]
                fn default() -> Self {
                    Self::MIN
                }
            }

            impl<const N: usize> Bits for UInt<$t, N> {
                #[allow(clippy::cast_possible_truncation)] // Supported UInt widths are at most 128.
                const BITS: u32 = {
                    let () = Self::VALID_WIDTH;
                    N as u32
                };

                #[inline]
                fn into_bits(self) -> u128 {
                    self.__bnb_into_bits()
                }

                #[inline]
                fn from_bits(raw: u128) -> Self {
                    Self::__bnb_from_bits(raw)
                }
            }

            impl<const N: usize> fmt::Debug for UInt<$t, N> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    write!(f, "u{N}({})", self.value)
                }
            }

            impl<const N: usize> fmt::Display for UInt<$t, N> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    fmt::Display::fmt(&self.value, f)
                }
            }

            impl_uint_radix_fmt!($t; LowerHex, UpperHex, Octal, Binary);

            impl<const N: usize> From<UInt<$t, N>> for $t {
                #[inline]
                fn from(v: UInt<$t, N>) -> $t {
                    v.value
                }
            }

            impl<const N: usize> TryFrom<$t> for UInt<$t, N> {
                type Error = WidthError;
                #[inline]
                fn try_from(value: $t) -> core::result::Result<Self, WidthError> {
                    Self::try_new(value)
                }
            }
        )*
    };
}

impl_uint!(u8, u16, u32, u64, u128);

impl_uint_try_from!(u8; u16, u32, u64, u128, usize);
impl_uint_try_from!(u16; u8, u32, u64, u128, usize);
impl_uint_try_from!(u32; u8, u16, u64, u128, usize);
impl_uint_try_from!(u64; u8, u16, u32, u128, usize);
impl_uint_try_from!(u128; u8, u16, u32, u64, usize);

// `macro_rules!` cannot build identifiers like `u5` from a literal, so the
// aliases are written out explicitly below (grouped by backing width). This is
// verbose but keeps the crate free of a `paste`-style dependency.
//
// The lower-case `uN` names intentionally mirror the standard `u8`/`u16` style
// (and `arbitrary-int`), so the camel-case lint is silenced here.
/// Type aliases `u1`..`u127`: each `uN` is an `N`-bit unsigned integer backed by
/// the smallest sufficient primitive (`UInt<u8, 5>` for `u5`, etc.). The native
/// widths (`u8`/`u16`/`u32`/`u64`/`u128`) are the standard library's.
#[rustfmt::skip]
#[allow(non_camel_case_types, missing_docs)] // the names are self-documenting
mod aliases {
    use super::UInt;

    // 1..=7 bits fit in a u8.
    pub type u1 = UInt<u8, 1>;   pub type u2 = UInt<u8, 2>;   pub type u3 = UInt<u8, 3>;
    pub type u4 = UInt<u8, 4>;   pub type u5 = UInt<u8, 5>;   pub type u6 = UInt<u8, 6>;
    pub type u7 = UInt<u8, 7>;

    // 9..=15 bits fit in a u16.
    pub type u9  = UInt<u16, 9>;  pub type u10 = UInt<u16, 10>; pub type u11 = UInt<u16, 11>;
    pub type u12 = UInt<u16, 12>; pub type u13 = UInt<u16, 13>; pub type u14 = UInt<u16, 14>;
    pub type u15 = UInt<u16, 15>;

    // 17..=31 bits fit in a u32.
    pub type u17 = UInt<u32, 17>; pub type u18 = UInt<u32, 18>; pub type u19 = UInt<u32, 19>;
    pub type u20 = UInt<u32, 20>; pub type u21 = UInt<u32, 21>; pub type u22 = UInt<u32, 22>;
    pub type u23 = UInt<u32, 23>; pub type u24 = UInt<u32, 24>; pub type u25 = UInt<u32, 25>;
    pub type u26 = UInt<u32, 26>; pub type u27 = UInt<u32, 27>; pub type u28 = UInt<u32, 28>;
    pub type u29 = UInt<u32, 29>; pub type u30 = UInt<u32, 30>; pub type u31 = UInt<u32, 31>;

    // 33..=63 bits fit in a u64.
    pub type u33 = UInt<u64, 33>; pub type u34 = UInt<u64, 34>; pub type u35 = UInt<u64, 35>;
    pub type u36 = UInt<u64, 36>; pub type u37 = UInt<u64, 37>; pub type u38 = UInt<u64, 38>;
    pub type u39 = UInt<u64, 39>; pub type u40 = UInt<u64, 40>; pub type u41 = UInt<u64, 41>;
    pub type u42 = UInt<u64, 42>; pub type u43 = UInt<u64, 43>; pub type u44 = UInt<u64, 44>;
    pub type u45 = UInt<u64, 45>; pub type u46 = UInt<u64, 46>; pub type u47 = UInt<u64, 47>;
    pub type u48 = UInt<u64, 48>; pub type u49 = UInt<u64, 49>; pub type u50 = UInt<u64, 50>;
    pub type u51 = UInt<u64, 51>; pub type u52 = UInt<u64, 52>; pub type u53 = UInt<u64, 53>;
    pub type u54 = UInt<u64, 54>; pub type u55 = UInt<u64, 55>; pub type u56 = UInt<u64, 56>;
    pub type u57 = UInt<u64, 57>; pub type u58 = UInt<u64, 58>; pub type u59 = UInt<u64, 59>;
    pub type u60 = UInt<u64, 60>; pub type u61 = UInt<u64, 61>; pub type u62 = UInt<u64, 62>;
    pub type u63 = UInt<u64, 63>;

    // 65..=127 bits fit in a u128.
    pub type u65 = UInt<u128, 65>;   pub type u66 = UInt<u128, 66>;   pub type u67 = UInt<u128, 67>;
    pub type u68 = UInt<u128, 68>;   pub type u69 = UInt<u128, 69>;   pub type u70 = UInt<u128, 70>;
    pub type u71 = UInt<u128, 71>;   pub type u72 = UInt<u128, 72>;   pub type u73 = UInt<u128, 73>;
    pub type u74 = UInt<u128, 74>;   pub type u75 = UInt<u128, 75>;   pub type u76 = UInt<u128, 76>;
    pub type u77 = UInt<u128, 77>;   pub type u78 = UInt<u128, 78>;   pub type u79 = UInt<u128, 79>;
    pub type u80 = UInt<u128, 80>;   pub type u81 = UInt<u128, 81>;   pub type u82 = UInt<u128, 82>;
    pub type u83 = UInt<u128, 83>;   pub type u84 = UInt<u128, 84>;   pub type u85 = UInt<u128, 85>;
    pub type u86 = UInt<u128, 86>;   pub type u87 = UInt<u128, 87>;   pub type u88 = UInt<u128, 88>;
    pub type u89 = UInt<u128, 89>;   pub type u90 = UInt<u128, 90>;   pub type u91 = UInt<u128, 91>;
    pub type u92 = UInt<u128, 92>;   pub type u93 = UInt<u128, 93>;   pub type u94 = UInt<u128, 94>;
    pub type u95 = UInt<u128, 95>;   pub type u96 = UInt<u128, 96>;   pub type u97 = UInt<u128, 97>;
    pub type u98 = UInt<u128, 98>;   pub type u99 = UInt<u128, 99>;   pub type u100 = UInt<u128, 100>;
    pub type u101 = UInt<u128, 101>; pub type u102 = UInt<u128, 102>; pub type u103 = UInt<u128, 103>;
    pub type u104 = UInt<u128, 104>; pub type u105 = UInt<u128, 105>; pub type u106 = UInt<u128, 106>;
    pub type u107 = UInt<u128, 107>; pub type u108 = UInt<u128, 108>; pub type u109 = UInt<u128, 109>;
    pub type u110 = UInt<u128, 110>; pub type u111 = UInt<u128, 111>; pub type u112 = UInt<u128, 112>;
    pub type u113 = UInt<u128, 113>; pub type u114 = UInt<u128, 114>; pub type u115 = UInt<u128, 115>;
    pub type u116 = UInt<u128, 116>; pub type u117 = UInt<u128, 117>; pub type u118 = UInt<u128, 118>;
    pub type u119 = UInt<u128, 119>; pub type u120 = UInt<u128, 120>; pub type u121 = UInt<u128, 121>;
    pub type u122 = UInt<u128, 122>; pub type u123 = UInt<u128, 123>; pub type u124 = UInt<u128, 124>;
    pub type u125 = UInt<u128, 125>; pub type u126 = UInt<u128, 126>; pub type u127 = UInt<u128, 127>;
}

pub use aliases::*;

#[cfg(test)]
mod unit {
    use alloc::vec::Vec;
    use core::any::type_name;

    use super::*;

    #[test]
    fn construction_and_range() {
        assert_eq!(u5::MAX.value(), 31);
        assert_eq!(u5::MIN.value(), 0);
        assert_eq!(u5::new(17).value(), 17);
        assert!(u5::try_new(31).is_ok());
        assert!(u5::try_new(32).is_err());
        // from_raw masks rather than rejects.
        assert_eq!(u5::from_raw(0xFF).value(), 31);
    }

    #[test]
    #[should_panic(expected = "value out of range")]
    fn new_panics_on_overflow() {
        let _ = u4::new(16);
    }

    #[test]
    fn bits_round_trip() {
        let v = u13::new(0x1ABC & 0x1FFF);
        assert_eq!(<u13 as Bits>::BITS, 13);
        assert_eq!(u13::from_bits(v.into_bits()), v);
        // into_bits exposes only the low N bits.
        assert!(v.into_bits() <= u128::from(u13::MAX.value()));
    }

    #[test]
    fn conversions() {
        let v = u12::new(0xABC);
        let raw: u16 = v.into();
        assert_eq!(raw, 0xABC);
        assert_eq!(u12::try_from(0xABCu16).unwrap(), v);
        assert!(u12::try_from(0x1000u16).is_err());
    }

    #[test]
    fn backing_primitive_from_uint() {
        assert_eq!(u8::from(u5::new(17)), 17);
        assert_eq!(u16::from(u12::new(0xABC)), 0xABC);
        assert_eq!(u32::from(u24::MAX), 0x00FF_FFFF);
        assert_eq!(u64::from(u48::MAX), 0xFFFF_FFFF_FFFF);
        assert_eq!(u128::from(u127::MAX), u128::MAX >> 1);
    }

    /// A primitive source type of the conversion matrix, whose value is read here
    /// without going through the conversion under test.
    trait Source: Copy + fmt::Debug + TryFrom<u128> {
        const MIN: Self;
        const MAX: Self;

        /// The mathematical value: `Ok` when non-negative, `Err` when negative.
        fn math(self) -> Result<u128, i128>;
    }

    macro_rules! impl_source {
        ($($s:ty),*) => {$(
            impl Source for $s {
                const MIN: Self = <$s>::MIN;
                const MAX: Self = <$s>::MAX;

                fn math(self) -> Result<u128, i128> {
                    u128::try_from(self)
                        .map_err(|_| i128::try_from(self).expect("a negative primitive fits i128"))
                }
            }
        )*};
    }
    impl_source!(
        u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
    );

    /// The largest value `bits` bits hold, derived without `UInt::MASK`.
    fn max_for(bits: u32) -> u128 {
        u128::MAX >> (128 - bits)
    }

    /// The values to try for one source type against a `bits`-wide target: zero, the
    /// target's maximum and both its neighbours where the source can hold them, and
    /// the source's own extremes.
    fn boundaries<S: Source>(bits: u32) -> Vec<S> {
        let max = max_for(bits);
        [Some(0), max.checked_sub(1), Some(max), max.checked_add(1)]
            .into_iter()
            .flatten()
            .filter_map(|value| S::try_from(value).ok())
            .chain([S::MIN, S::MAX])
            .collect()
    }

    /// Asserts that `value` converts into `U` exactly when it fits in `U`'s width, and
    /// is otherwise rejected with the variant and value that say why.
    fn check<U, S>(value: S)
    where
        U: TryFrom<S, Error = WidthError> + Bits + fmt::Debug,
        S: Source,
    {
        let bits = U::BITS;
        let expected = match value.math() {
            Ok(value) if value <= max_for(bits) => Ok(value),
            Ok(value) => Err(WidthError::ValueTooLarge { value, bits }),
            Err(value) => Err(WidthError::Negative { value, bits }),
        };
        assert_eq!(
            U::try_from(value).map(Bits::into_bits),
            expected,
            "{} {value:?} -> {}: Ok exactly when the value fits in {bits} bits",
            type_name::<S>(),
            type_name::<U>(),
        );
    }

    macro_rules! check_matrix {
        ($($target:ty),* $(,)?) => {$(
            check_matrix!(
                @sources $target;
                u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
            );
        )*};
        (@sources $target:ty; $($source:ty),*) => {$(
            for value in boundaries::<$source>(<$target>::BITS) {
                check::<$target, $source>(value);
            }
        )*};
    }

    #[test]
    fn try_from_accepts_exactly_the_values_that_fit() {
        // Per backing: a width strictly inside it and its full width; then the
        // narrowest and widest aliases and a width narrower than a smaller primitive.
        check_matrix!(
            u5,
            UInt<u8, 8>,
            u12,
            UInt<u16, 16>,
            u24,
            UInt<u32, 32>,
            u48,
            UInt<u64, 64>,
            u100,
            UInt<u128, 128>,
            u1,
            u127,
            UInt<u64, 7>,
        );
    }

    #[test]
    fn try_from_checks_the_value_a_cast_would_have_truncated() {
        // 65_541 is 0x1_0005: `as u16` leaves 5, which fits in 12 bits.
        assert_eq!(
            u12::try_from(65_541_usize),
            Err(WidthError::ValueTooLarge {
                value: 65_541,
                bits: 12
            })
        );
        // 256 is 0x100: `as u8` leaves 0.
        assert_eq!(
            u4::try_from(256_u16),
            Err(WidthError::ValueTooLarge {
                value: 256,
                bits: 4
            })
        );
        // -1 is all ones: `as u64` leaves `u64::MAX`, which masks to a valid u48.
        assert_eq!(
            u48::try_from(-1_i8),
            Err(WidthError::Negative {
                value: -1,
                bits: 48
            })
        );
    }

    #[test]
    fn try_from_rejects_negative_sources_with_their_value() {
        macro_rules! rejects {
            ($($s:ty => $min:expr),* $(,)?) => {$(
                assert_eq!(
                    u12::try_from(-1 as $s),
                    Err(WidthError::Negative { value: -1, bits: 12 }),
                    "-1 as {}",
                    stringify!($s)
                );
                assert_eq!(
                    UInt::<u128, 128>::try_from(<$s>::MIN),
                    Err(WidthError::Negative { value: $min, bits: 128 }),
                    "{}::MIN",
                    stringify!($s)
                );
            )*};
        }
        rejects!(
            i8 => -128,
            i16 => -32_768,
            i32 => -2_147_483_648,
            i64 => -9_223_372_036_854_775_808,
            i128 => i128::MIN,
            isize => i128::try_from(isize::MIN).unwrap(),
        );
    }

    #[test]
    fn try_from_handles_the_128_bit_extremes() {
        assert_eq!(
            u5::try_from(u128::MAX),
            Err(WidthError::ValueTooLarge {
                value: u128::MAX,
                bits: 5
            })
        );
        assert_eq!(
            u127::try_from(u128::MAX),
            Err(WidthError::ValueTooLarge {
                value: u128::MAX,
                bits: 127
            })
        );
        assert_eq!(
            UInt::<u128, 128>::try_from(u128::MAX).map(UInt::<u128, 128>::value),
            Ok(u128::MAX)
        );
        // i128::MAX is 2^127 - 1: exactly u127::MAX, one bit too many for u126.
        assert_eq!(u127::try_from(i128::MAX), Ok(u127::MAX));
        assert_eq!(
            u126::try_from(i128::MAX),
            Err(WidthError::ValueTooLarge {
                value: u128::MAX >> 1,
                bits: 126
            })
        );
        assert_eq!(
            u1::try_from(i128::MIN),
            Err(WidthError::Negative {
                value: i128::MIN,
                bits: 1
            })
        );
    }

    #[test]
    fn try_from_pointer_sized_sources_on_any_pointer_width() {
        // Widened here with the checked conversions, so the expectations hold
        // whether `usize` is 32 or 64 bits wide.
        let usize_max = u128::try_from(usize::MAX).unwrap();
        let isize_max = u128::try_from(isize::MAX).unwrap();
        let isize_min = i128::try_from(isize::MIN).unwrap();

        assert_eq!(u12::try_from(4095_usize), Ok(u12::MAX));
        assert_eq!(u12::try_from(4095_isize), Ok(u12::MAX));
        assert_eq!(
            u12::try_from(usize::MAX),
            Err(WidthError::ValueTooLarge {
                value: usize_max,
                bits: 12
            })
        );
        assert_eq!(
            u12::try_from(isize::MAX),
            Err(WidthError::ValueTooLarge {
                value: isize_max,
                bits: 12
            })
        );
        assert_eq!(
            u12::try_from(isize::MIN),
            Err(WidthError::Negative {
                value: isize_min,
                bits: 12
            })
        );
        // A 128-bit target holds every `usize`; a 127-bit one every `isize::MAX`.
        assert_eq!(
            UInt::<u128, 128>::try_from(usize::MAX).map(UInt::<u128, 128>::value),
            Ok(usize_max)
        );
        assert_eq!(u127::try_from(isize::MAX).map(u127::value), Ok(isize_max));
    }

    #[test]
    fn try_from_infers_literals_and_typed_sources() -> Result<(), WidthError> {
        // A bare literal falls back to i32, which has an impl.
        assert_eq!(u12::try_from(5)?.value(), 5);
        assert_eq!(u48::try_from(5)?.value(), 5);
        let x: u12 = 7_u16.try_into()?;
        assert_eq!(x.value(), 7);
        let y: Result<u12, _> = 7.try_into();
        assert_eq!(y?.value(), 7);
        let typed: Vec<u12> = [1_u32, 2, 4095]
            .into_iter()
            .map(u12::try_from)
            .collect::<Result<_, _>>()?;
        assert_eq!(typed, [u12::new(1), u12::new(2), u12::MAX]);
        let bare: Result<Vec<u12>, _> = [1, 2, 4096].into_iter().map(u12::try_from).collect();
        assert_eq!(
            bare,
            Err(WidthError::ValueTooLarge {
                value: 4096,
                bits: 12
            })
        );
        // Out-of-range literals are run-time errors, not compile errors.
        assert_eq!(
            u12::try_from(-1),
            Err(WidthError::Negative {
                value: -1,
                bits: 12
            })
        );
        assert_eq!(
            u12::try_from(70_000),
            Err(WidthError::ValueTooLarge {
                value: 70_000,
                bits: 12
            })
        );
        Ok(())
    }

    #[test]
    fn widest_widths_work() {
        assert_eq!(u127::MAX.value(), (1u128 << 127) - 1);
        assert_eq!(u1::MAX.value(), 1);
    }

    #[test]
    fn display_renders_the_plain_value() {
        use alloc::string::ToString;
        assert_eq!(u12::new(0xABC).to_string(), "2748");
        assert_eq!(u5::new(0).to_string(), "0");
    }

    #[test]
    fn radix_formatting_matches_the_backing_primitive() {
        use alloc::format;

        // Each spec renders the UInt and its backing value identically.
        macro_rules! same_as_primitive {
            ($uint:expr, $prim:expr; $($spec:literal),* $(,)?) => {$(
                assert_eq!(
                    format!($spec, $uint),
                    format!($spec, $prim),
                    "format spec {:?} for {:?}",
                    $spec,
                    $uint
                );
            )*};
        }
        macro_rules! every_spec {
            ($uint:expr, $prim:expr) => {
                same_as_primitive!(
                    $uint, $prim;
                    "{:x}", "{:X}", "{:o}", "{:b}",
                    "{:#x}", "{:#X}", "{:#o}", "{:#b}",
                    "{:#06x}", "{:08b}", "{:012o}", "{:#010X}",
                    "{:>8x}|", "{:<8X}|", "{:^9o}|", "{:*>#12b}|", "{:+x}",
                );
            };
        }
        every_spec!(u5::new(17), 17_u8);
        every_spec!(u12::new(0xABC), 0xABC_u16);
        every_spec!(u24::MAX, 0x00FF_FFFF_u32);
        every_spec!(u48::new(0xDEAD_BEEF_F00D), 0xDEAD_BEEF_F00D_u64);
        every_spec!(u127::MAX, u128::MAX >> 1);
        every_spec!(u1::MIN, 0_u8);

        // Anchors that do not depend on the primitive's own formatter.
        let v = u12::new(0xABC);
        assert_eq!(format!("{v:x}"), "abc");
        assert_eq!(format!("{v:X}"), "ABC");
        assert_eq!(format!("{v:o}"), "5274");
        assert_eq!(format!("{v:b}"), "101010111100");
        assert_eq!(format!("{v:#06x}"), "0x0abc");
        assert_eq!(format!("{:08b}", u5::new(17)), "00010001");
    }

    #[test]
    fn debug_shows_the_width_and_value() {
        use alloc::format;
        assert_eq!(format!("{:?}", u4::new(0xA)), "u4(10)");
        assert_eq!(format!("{:?}", u12::new(0xABC)), "u12(2748)");
    }
}
