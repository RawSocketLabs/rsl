//! [`WireLen<T>`] — a length/count field that auto-derives by default but can be pinned.
//!
//! A wire length or count (a UDP length, a DNS `qdcount`, an `rdlength`) normally equals the
//! real size of what it describes, but a **dual-use** codec must also let you write a value
//! that *disagrees* (a forged/malformed frame). `WireLen<T>` is that field: it is either
//!
//! - [`Auto`](WireLen::Auto) — "derive me at encode" (the [`Default`]), or
//! - [`Set`](WireLen::Set) — "write exactly this value" (an override, or a decoded value).
//!
//! On **decode** it always yields `Set(decoded)`, so `decode → encode` stays byte-identical
//! (a forged length survives a round-trip). On **encode**, an `Auto` is resolved from a
//! declared target via the `#[bw(auto = count(x) | bytes(x))]` field directive (same-struct
//! target) or the `#[bin(auto_len(path = count(field), …))]` struct directive (cross-struct);
//! a `Set` is written as-is. So plain `to_bytes()` is correct by default, yet
//! `WireLen::set(n)` deliberately deviates. Derivation is checked ([`CountPrefix`]) — an
//! oversized length is a [`BitError`], never a silent truncation.

use crate::bitstream::{BitDecode, BitEncode, BitError, CountPrefix, FixedBitLen, Sink, Source};
use crate::field::Bits;

/// A wire length/count field: [`Auto`](Self::Auto)-derived by default, or an explicit
/// [`Set`](Self::Set) override. See the [module docs](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum WireLen<T> {
    /// Derive this length from its declared target when the message is encoded.
    #[default]
    Auto,
    /// Write exactly this value — a deliberate override, or the value read on decode.
    Set(T),
}

impl<T> WireLen<T> {
    /// An auto-deriving length (the default). Equivalent to [`WireLen::Auto`].
    #[must_use]
    pub fn auto() -> Self {
        WireLen::Auto
    }

    /// A pinned, explicit length. Equivalent to [`WireLen::Set`] — the dual-use override.
    #[must_use]
    pub fn set(value: T) -> Self {
        WireLen::Set(value)
    }

    /// Whether this length auto-derives (has no explicit value yet).
    #[must_use]
    pub fn is_auto(&self) -> bool {
        matches!(self, WireLen::Auto)
    }

    /// The explicit value, or `None` if it auto-derives. A decoded `WireLen` is always
    /// `Some`; a freshly-built `Auto` is `None` until resolved at encode.
    #[must_use]
    pub fn get(&self) -> Option<&T> {
        match self {
            WireLen::Set(v) => Some(v),
            WireLen::Auto => None,
        }
    }
}

impl<T: CountPrefix> WireLen<T> {
    /// The value as a `usize` element count (for `#[br(count = …)]` on the decode side,
    /// where a `WireLen` is always [`Set`](Self::Set)). An unresolved [`Auto`](Self::Auto)
    /// reads as `0` (it never occurs on decode).
    #[must_use]
    pub fn to_count(&self) -> usize {
        match self {
            WireLen::Set(v) => v.to_count(),
            WireLen::Auto => 0,
        }
    }

    /// Resolve an [`Auto`](Self::Auto) to the checked count of `len` (leaving a
    /// [`Set`](Self::Set) untouched) — the encode-time derivation the macro emits.
    ///
    /// # Errors
    /// [`BitError`] if `len` exceeds `T`'s range (a checked conversion, never a truncation).
    pub fn resolve_count(&self, len: usize) -> Result<Self, BitError> {
        match self {
            WireLen::Set(v) => Ok(WireLen::Set(*v)),
            WireLen::Auto => Ok(WireLen::Set(T::try_from_len(len).map_err(BitError::from)?)),
        }
    }
}

impl<T: Bits> BitDecode for WireLen<T> {
    #[inline]
    fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
        Ok(WireLen::Set(r.read::<T>()?))
    }
}

impl<T: Bits> BitEncode for WireLen<T> {
    #[inline]
    fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
        match self {
            WireLen::Set(v) => w.write(*v),
            WireLen::Auto => Err(BitError::convert(
                "unresolved `WireLen::Auto`: this field needs an `auto = …` / `auto_len(…)` \
                 directive to derive its value, or set it explicitly with `WireLen::set(n)`"
                    .into(),
                w.bit_pos(),
            )),
        }
    }
}

/// A `WireLen<T>` occupies exactly `T`'s width, so it stays a fixed-width field (a DNS
/// header with `WireLen<u16>` counts is still 12 bytes).
impl<T: Bits> FixedBitLen for WireLen<T> {
    const BIT_LEN: u32 = <T as Bits>::BITS;
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::bitstream::{BitReader, BitWriter};

    #[test]
    fn default_is_auto() {
        assert_eq!(WireLen::<u16>::default(), WireLen::Auto);
        assert!(WireLen::<u16>::auto().is_auto());
        assert!(!WireLen::set(5u16).is_auto());
    }

    #[test]
    fn decode_yields_set() {
        let mut r = BitReader::new(&[0x12, 0x34]);
        let v = WireLen::<u16>::bit_decode(&mut r).unwrap();
        assert_eq!(v, WireLen::Set(0x1234));
        assert_eq!(v.get(), Some(&0x1234));
        assert_eq!(v.to_count(), 0x1234);
    }

    #[test]
    fn encode_set_writes_the_value_auto_errors() {
        let mut w = BitWriter::new();
        WireLen::set(0xABCDu16).bit_encode(&mut w).unwrap();
        assert_eq!(w.into_bytes(), [0xAB, 0xCD]);

        let mut w = BitWriter::new();
        assert!(WireLen::<u16>::auto().bit_encode(&mut w).is_err());
    }

    #[test]
    fn resolve_count_fills_auto_leaves_set_and_checks_overflow() {
        assert_eq!(
            WireLen::<u16>::auto().resolve_count(7).unwrap(),
            WireLen::Set(7)
        );
        assert_eq!(
            WireLen::set(9u16).resolve_count(7).unwrap(),
            WireLen::Set(9)
        ); // Set wins
        // 300 doesn't fit a u8 → checked error, not a truncation to 44.
        assert!(WireLen::<u8>::auto().resolve_count(300).is_err());
    }

    #[test]
    fn fixed_bit_len_matches_the_inner_width() {
        assert_eq!(<WireLen<u16> as FixedBitLen>::BIT_LEN, 16);
        assert_eq!(<WireLen<u32> as FixedBitLen>::BIT_LEN, 32);
    }
}
