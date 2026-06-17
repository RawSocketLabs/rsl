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

use core::fmt;

use crate::error::{Error, Result};
use crate::field::Bits;

/// An unsigned integer constrained to `N` bits, backed by primitive `T`.
///
/// Use the aliases ([`u5`], [`u13`], …) rather than naming `T` directly. The
/// value is always in `0..=MAX` (`MAX == 2^N - 1`); construct with
/// [`new`](UInt::new) (panicking) or [`try_new`](UInt::try_new) (checked), and
/// read it back with [`value`](UInt::value).
///
/// # Examples
///
/// ```
/// use bnb::u5;
///
/// let a = u5::new(17);              // checked; panics if > 31
/// assert_eq!(a.value(), 17);
/// assert!(u5::try_new(32).is_err()); // out of range -> Err, no panic
/// assert_eq!(u5::from_raw(0xFF).value(), 31); // masks to the low 5 bits
/// assert_eq!(u5::MAX.value(), 31);
/// assert_eq!(u5::MIN.value(), 0);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UInt<T, const N: usize> {
    value: T,
}

macro_rules! impl_uint {
    ($($t:ty),* $(,)?) => {
        $(
            impl<const N: usize> UInt<$t, N> {
                /// The number of bits.
                pub const BITS: usize = N;

                /// A mask with the low `N` bits set.
                pub const MASK: $t = if N >= <$t>::BITS as usize {
                    <$t>::MAX
                } else {
                    ((1 as $t) << N) - 1
                };

                /// The largest representable value (`2^N - 1`).
                pub const MAX: Self = Self { value: Self::MASK };

                /// The zero value.
                pub const MIN: Self = Self { value: 0 };

                /// Creates a value, panicking if it does not fit in `N` bits.
                ///
                /// # Panics
                /// Panics if `value > MAX`.
                #[inline]
                pub const fn new(value: $t) -> Self {
                    assert!(value <= Self::MASK, "value out of range for this width");
                    Self { value }
                }

                /// Creates a value, or [`Error::ValueTooLarge`] if it does not
                /// fit in `N` bits.
                #[inline]
                pub fn try_new(value: $t) -> Result<Self> {
                    if value <= Self::MASK {
                        Ok(Self { value })
                    } else {
                        Err(Error::ValueTooLarge {
                            value: value as u128,
                            bits: N as u32,
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
                    self.value
                }
            }

            impl<const N: usize> Default for UInt<$t, N> {
                #[inline]
                fn default() -> Self {
                    Self { value: 0 }
                }
            }

            impl<const N: usize> Bits for UInt<$t, N> {
                const BITS: u32 = N as u32;

                #[inline]
                fn into_bits(self) -> u128 {
                    self.value as u128
                }

                #[inline]
                fn from_bits(raw: u128) -> Self {
                    Self::from_raw(raw as $t)
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

            impl<const N: usize> From<UInt<$t, N>> for $t {
                #[inline]
                fn from(v: UInt<$t, N>) -> $t {
                    v.value
                }
            }

            impl<const N: usize> TryFrom<$t> for UInt<$t, N> {
                type Error = Error;
                #[inline]
                fn try_from(value: $t) -> Result<Self> {
                    Self::try_new(value)
                }
            }
        )*
    };
}

impl_uint!(u8, u16, u32, u64, u128);

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
        assert!(v.into_bits() <= u13::MAX.value() as u128);
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
    fn widest_widths_work() {
        assert_eq!(u127::MAX.value(), (1u128 << 127) - 1);
        assert_eq!(u1::MAX.value(), 1);
    }
}
