//! The field-value abstraction shared by every bit-packable type.
//!
//! [`Bits`] is the universal trait: a value that occupies a fixed number of bits
//! inside a bitfield. `bool`, the primitive unsigned integers, the
//! [`UInt`](crate::int::UInt) arbitrary-width integers, nested `#[bitfield]`
//! structs, and `#[derive(BitEnum)]` enums all implement it, so they compose as
//! fields. `u128` is the universal carrier — wide enough for any field this
//! crate supports (the maximum width is 128 bits).

/// Byte order of a bitfield's backing integer when it is serialized.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ByteOrder {
    /// Most-significant byte first (network order).
    Big,
    /// Least-significant byte first.
    Little,
}

/// Bit packing order within a bitfield: does the first declared field occupy the
/// most-significant or least-significant bits of the backing integer.
///
/// Most network protocols (and the ASCII-art layouts in their RFCs) are
/// most-significant-first, so [`Msb`](BitOrder::Msb) is the crate default.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BitOrder {
    /// First field in the high bits (network / RFC-diagram order). The default.
    Msb,
    /// First field in the low bits.
    Lsb,
}

/// A value that occupies a fixed number of bits within a bitfield.
///
/// The contract: [`into_bits`](Bits::into_bits) yields the value in the low
/// [`BITS`](Bits::BITS) bits of a `u128` (higher bits zero), and
/// [`from_bits`](Bits::from_bits) reconstructs from the low `BITS` bits of its
/// argument (higher bits ignored). Implementations must round-trip:
/// `T::from_bits(x.into_bits()) == x` for every representable `x`.
///
/// `bool`, the primitive unsigned integers, and the [`UInt`](crate::UInt) types
/// implement it out of the box; `#[bitfield]` and `#[derive(BitEnum)]` generate
/// impls so those types nest as fields too.
///
/// ```
/// use bits::Bits;
///
/// assert_eq!(<u8 as Bits>::BITS, 8);
/// assert_eq!(0xABu8.into_bits(), 0xAB);
/// assert_eq!(u8::from_bits(0x1FF), 0xFF); // from_bits truncates to the width
/// assert!(!bool::from_bits(0b10)); // only the low bit is read
/// ```
pub trait Bits: Copy {
    /// The number of bits this value occupies on the wire.
    const BITS: u32;

    /// This value as the low [`BITS`](Bits::BITS) bits of a `u128`.
    fn into_bits(self) -> u128;

    /// Reconstruct from the low [`BITS`](Bits::BITS) bits of `raw`; any higher
    /// bits are ignored.
    fn from_bits(raw: u128) -> Self;
}

impl Bits for bool {
    const BITS: u32 = 1;

    #[inline]
    fn into_bits(self) -> u128 {
        self as u128
    }

    #[inline]
    fn from_bits(raw: u128) -> Self {
        (raw & 1) != 0
    }
}

/// Implements [`Bits`] for the primitive unsigned integers (full width).
macro_rules! impl_bits_for_primitive {
    ($($t:ty),* $(,)?) => {
        $(
            impl Bits for $t {
                const BITS: u32 = <$t>::BITS;

                #[inline]
                fn into_bits(self) -> u128 {
                    self as u128
                }

                #[inline]
                fn from_bits(raw: u128) -> Self {
                    // `as` truncates to the low `BITS` bits, which is exactly the
                    // masking the contract requires.
                    raw as $t
                }
            }
        )*
    };
}

impl_bits_for_primitive!(u8, u16, u32, u64, u128);

/// The seam every `#[bitfield]` struct implements — the stable interface a codec
/// (binrw today, an in-house codec later) builds on, independent of how the
/// fields are accessed.
///
/// A bitfield is a thin wrapper over a single backing unsigned integer; this
/// trait exposes that backing plus the declared layout metadata. The generated
/// type also provides inherent `to_be_bytes`/`to_le_bytes`/`from_be_bytes`/
/// `from_le_bytes` for allocation-free (de)serialization.
pub trait Bitfield: Bits + Sized {
    /// The backing primitive integer (`u8`, `u16`, `u32`, `u64`, or `u128`).
    type Backing: Copy;

    /// The declared total bit width (may be less than the backing's width, the
    /// remainder being reserved/padding).
    const WIDTH: u32;

    /// The byte order in which the backing integer is serialized.
    const BYTE_ORDER: ByteOrder;

    /// The bit packing order of the declared fields.
    const BIT_ORDER: BitOrder;

    /// The raw backing integer.
    fn to_raw(self) -> Self::Backing;

    /// Construct directly from a raw backing integer (no validation — the
    /// dual-use escape hatch).
    fn from_raw(raw: Self::Backing) -> Self;
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn bool_occupies_one_bit() {
        assert_eq!(<bool as Bits>::BITS, 1);
        assert_eq!(true.into_bits(), 1);
        assert_eq!(false.into_bits(), 0);
        assert!(bool::from_bits(1));
        assert!(!bool::from_bits(0));
        // Only the low bit is consulted.
        assert!(!bool::from_bits(0b1110));
        assert!(bool::from_bits(0b1111));
    }

    #[test]
    fn primitives_round_trip_and_truncate() {
        assert_eq!(<u8 as Bits>::BITS, 8);
        assert_eq!(<u32 as Bits>::BITS, 32);
        assert_eq!(0xABu8.into_bits(), 0xAB);
        // from_bits truncates to the type's width.
        assert_eq!(u8::from_bits(0x1FF), 0xFF);
        assert_eq!(u16::from_bits(0x3_1234), 0x1234);
        let v: u128 = 0xDEAD_BEEF_DEAD_BEEF_DEAD_BEEF_DEAD_BEEF;
        assert_eq!(u128::from_bits(v.into_bits()), v);
    }
}
