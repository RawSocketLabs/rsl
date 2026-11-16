//! [`CountPrefix`]: checked length to wire-prefix conversions.

use crate::field::Bits;

use super::sealed;

/// Checked length ⇄ wire-prefix conversions: the types usable as a length/count prefix —
/// by the [`#[brw(count_prefix = <Ty>)]`](macro@crate::bin) directive and by the
/// [`codecs::prefixed`](crate::codecs::prefixed) string codec.
///
/// The prefix is computed from `len()` on encode and turned back into an element count on
/// decode. `try_from_count` is **checked and never truncates**: the length is widened (never
/// narrowed) before the range compare, so 300 elements against a `u8` prefix is
/// [`WidthError::ValueTooLarge`] — not a silently wrapped `44`.
///
/// [`WidthError::ValueTooLarge`]: crate::WidthError::ValueTooLarge
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be a `count_prefix` type",
    note = "supported prefix types: u8, u16, u32, u64, u128 and the arbitrary-width `uN` aliases (e.g. `u12`)",
    note = "this trait is sealed — the supported prefix types are built in"
)]
pub trait CountPrefix: Bits + sealed::Sealed {
    /// The prefix for a collection of `len` elements.
    ///
    /// # Errors
    /// Returns [`crate::WidthError::ValueTooLarge`] when `len` exceeds the prefix's range.
    fn try_from_count(len: usize) -> core::result::Result<Self, crate::error::WidthError>
    where
        Self: Sized;

    /// The prefix as an element count.
    ///
    /// Counts too large for this host saturate at `usize::MAX`, never wrap to a smaller
    /// frame. Decoders must bound allocation and require input progress per element.
    fn to_count(self) -> usize;
}

macro_rules! count_prefix_prim {
    ($($t:ty),* $(,)?) => {$(
        impl sealed::Sealed for $t {}

        impl CountPrefix for $t {
            #[inline]
            fn try_from_count(len: usize) -> core::result::Result<Self, crate::error::WidthError> {
                <$t>::try_from(len).map_err(|_| crate::error::WidthError::ValueTooLarge {
                    value: len as u128,
                    bits: <$t>::BITS,
                })
            }

            #[inline]
            fn to_count(self) -> usize {
                usize::try_from(self).unwrap_or(usize::MAX)
            }
        }
    )*};
}
count_prefix_prim!(u8, u16, u32, u64, u128);

macro_rules! count_prefix_uint {
    ($($t:ty),* $(,)?) => {$(
        impl<const N: usize> sealed::Sealed for crate::int::UInt<$t, N> {}

        impl<const N: usize> CountPrefix for crate::int::UInt<$t, N> {
            #[inline]
            fn try_from_count(len: usize) -> core::result::Result<Self, crate::error::WidthError> {
                // Widen-then-compare: narrowing first (`len as $t`) would truncate
                // *before* the range check and let an oversized length slip through.
                let wide = len as u128;
                if wide > u128::from(Self::MASK) {
                    return Err(crate::error::WidthError::ValueTooLarge {
                        value: wide,
                        bits: Self::BITS,
                    });
                }
                Ok(Self::from_raw(<$t>::try_from(len).expect("count was checked against the UInt mask")))
            }

            #[inline]
            fn to_count(self) -> usize {
                usize::try_from(self.value()).unwrap_or(usize::MAX)
            }
        }
    )*};
}
count_prefix_uint!(u8, u16, u32, u64, u128);

#[cfg(test)]
mod unit {
    use crate::{u4, u12};

    use super::*;

    #[test]
    fn count_prefix_primitive_boundaries() {
        // In-range: exact fit at the top of the range.
        assert_eq!(u8::try_from_count(255).unwrap(), 255u8);
        // Out of range: checked, not wrapped.
        assert_eq!(
            u8::try_from_count(256).unwrap_err(),
            crate::error::WidthError::ValueTooLarge {
                value: 256,
                bits: 8
            }
        );
        assert_eq!(u16::try_from_count(65_535).unwrap(), 65_535u16);
        assert_eq!(u8::to_count(200), 200);
        assert_eq!(u32::to_count(70_000), 70_000);
    }

    #[test]
    fn count_prefix_uint_boundaries() {
        // u12: 4095 fits, 4096 does not.
        assert_eq!(u12::try_from_count(4095).unwrap(), u12::new(4095));
        assert_eq!(
            u12::try_from_count(4096).unwrap_err(),
            crate::error::WidthError::ValueTooLarge {
                value: 4096,
                bits: 12
            }
        );
        assert_eq!(u12::new(4095).to_count(), 4095);
    }

    #[test]
    fn count_prefix_uint_never_truncates_before_the_check() {
        // 300 as u8 would wrap to 44 — a masked `from_raw`/`try_new(len as u8)` path
        // would then accept it. The widen-then-compare must reject instead.
        assert_eq!(
            u4::try_from_count(300).unwrap_err(),
            crate::error::WidthError::ValueTooLarge {
                value: 300,
                bits: 4
            }
        );
    }

    #[test]
    fn count_prefix_round_trips() {
        for len in [0usize, 1, 15, 255, 4095] {
            assert_eq!(u16::try_from_count(len).unwrap().to_count(), len);
            assert_eq!(u12::try_from_count(len).unwrap().to_count(), len);
        }
    }
}
