//! Typed bit/byte amounts for positioning directives and the skip/align helpers.

use super::{BitError, Sink, Source, sealed};

/// A typed bit/byte amount for positioning directives — `4.bits()`, `3.bytes()` —
/// resolving to a bit count. Bring it in with `use bnb::prelude::*`.
///
/// # Examples
///
/// ```
/// use bnb::prelude::*;
/// assert_eq!(4u32.bits(), 4);
/// assert_eq!(3u32.bytes(), 24);
/// ```
///
/// Used by the positioning directives, e.g. `#[br(pad_before = 2u32.bytes())]` — see
/// [`guide::directives`](crate::guide::directives).
/// The result retains the low 32 bits, including multiplication by eight for `bytes`;
/// negative and overflowing amounts wrap. For untrusted lengths, validate the range
/// before converting: this convenience trait is not a checked length conversion.
pub trait BitAmount: Copy + sealed::Sealed {
    /// This many **bits**.
    fn bits(self) -> u32;
    /// This many **bytes** (× 8 bits).
    fn bytes(self) -> u32;
}

macro_rules! impl_bit_amount {
    ($($t:ty),*) => {$(
        impl BitAmount for $t {
            // Preserve the existing signed/unsigned low-32-bit conversion contract.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless)]
            fn bits(self) -> u32 { self as u32 }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless)]
            fn bytes(self) -> u32 { (self as u32).wrapping_mul(8) }
        }
    )*};
}
impl_bit_amount!(u8, u16, u32, u64, usize, i16, i32, i64, isize);
// Seal the `BitAmount` types that aren't already sealed by `CountPrefix`
// (`u8, u16, u32, u64` get their `Sealed` impl from `count_prefix_prim!`).
impl sealed::Sealed for usize {}
impl sealed::Sealed for i16 {}
impl sealed::Sealed for i32 {}
impl sealed::Sealed for i64 {}
impl sealed::Sealed for isize {}

/// Skips `bits` forward (consuming and discarding) — backs `#[br(pad_before/after)]`.
///
/// # Errors
/// Propagates the source's [`BitError`].
#[doc(hidden)]
pub fn skip_read<S: Source>(r: &mut S, bits: u32) -> Result<(), BitError> {
    let mut left = bits;
    while left > 0 {
        let n = left.min(128);
        r.read_bits(n)?;
        left -= n;
    }
    Ok(())
}

/// Writes `bits` zero bits forward — the write dual of [`skip_read`].
///
/// # Errors
/// Propagates the sink's [`BitError`].
#[doc(hidden)]
pub fn skip_write<K: Sink>(w: &mut K, bits: u32) -> Result<(), BitError> {
    let mut left = bits;
    while left > 0 {
        let n = left.min(128);
        w.write_bits(0, n)?;
        left -= n;
    }
    Ok(())
}

/// Skips forward to the next byte boundary — backs `#[br(align_before/after)]`.
///
/// # Errors
/// Propagates the source's [`BitError`].
#[doc(hidden)]
pub fn align_read<S: Source>(r: &mut S) -> Result<(), BitError> {
    let pad = (8 - (r.bit_pos() % 8)) % 8;
    skip_read(
        r,
        u32::try_from(pad).expect("alignment padding is below eight bits"),
    )
}

/// Pads with zero bits to the next byte boundary — the write dual of [`align_read`].
///
/// # Errors
/// Propagates the sink's [`BitError`].
#[doc(hidden)]
pub fn align_write<K: Sink>(w: &mut K) -> Result<(), BitError> {
    let pad = (8 - (w.bit_pos() % 8)) % 8;
    skip_write(
        w,
        u32::try_from(pad).expect("alignment padding is below eight bits"),
    )
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn positioning_amounts_keep_low_32_bits_without_debug_only_panics() {
        assert_eq!(u32::MAX.bytes(), 0xffff_fff8);
        assert_eq!((-1i32).bits(), u32::MAX);
        assert_eq!((-1i32).bytes(), 0xffff_fff8);
        assert_eq!((1u64 << 32).bits(), 0);
        assert_eq!((1u64 << 29).bytes(), 0);
    }
}
