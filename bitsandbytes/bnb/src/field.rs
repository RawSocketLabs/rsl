//! The field-value abstraction shared by every bit-packable type.
//!
//! [`Bits`] is the universal trait: a value that occupies a fixed number of bits
//! inside a bitfield. `bool`, the primitive unsigned integers, the
//! [`UInt`](crate::int::UInt) arbitrary-width integers, nested `#[bitfield]`
//! structs, and `#[derive(BitEnum)]` enums all implement it, so they compose as
//! fields. `u128` is the universal carrier — wide enough for any field this
//! crate supports (the maximum width is 128 bits).

/// Byte order of a bitfield's backing integer when it is serialized.
///
/// # Examples
///
/// ```
/// use bnb::ByteOrder;
/// assert_eq!(ByteOrder::default(), ByteOrder::Big); // network order is the default
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum ByteOrder {
    /// Most-significant byte first (network order). The default.
    #[default]
    Big,
    /// Least-significant byte first.
    Little,
}

/// Bit packing order within a bitfield: does the first declared field occupy the
/// most-significant or least-significant bits of the backing integer.
///
/// Most network protocols (and the ASCII-art layouts in their RFCs) are
/// most-significant-first, so [`Msb`](BitOrder::Msb) is the crate default.
///
/// # Examples
///
/// ```
/// use bnb::BitOrder;
/// assert_eq!(BitOrder::default(), BitOrder::Msb); // first field in the high bits
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum BitOrder {
    /// First field in the high bits (network / RFC-diagram order). The default.
    #[default]
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
/// # Const dispatch (`#[bitfield]` field types)
///
/// The accessors `#[bitfield]` generates are `const fn`. A `const fn` cannot call
/// trait methods on stable Rust, so the generated code does not go through this
/// trait: `bool` and the primitive unsigned integers are converted inline, and
/// every other field type is called through a pair of inherent `const fn`s with
/// the same contract as `into_bits`/`from_bits`. [`UInt`](crate::UInt) and every
/// `#[bitfield]`/`#[derive(BitEnum)]`/`#[bitflags]` type provides the pair
/// automatically (their `Bits` impls delegate to it, so the two can never
/// disagree). **Implement a hand-written field type with
/// [`impl_bits!`](macro@crate::impl_bits)**, which emits the trait impl and the
/// inherent pair from one definition — never write the pair by hand. The trait
/// alone still suffices everywhere else (the `#[bin]` codec and the bitstream
/// derives). Field types must be named directly — a `type` alias of a primitive
/// is not recognized by the inline conversion.
///
/// ```
/// use bnb::Bits;
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

/// Implements [`Bits`] for a hand-written field type from one pair of `const fn`
/// conversion bodies — the supported way to make a custom type usable as a
/// `#[bitfield]` field.
///
/// The accessors `#[bitfield]` generates are `const fn`, and a `const fn` cannot
/// call trait methods on stable Rust — so alongside its [`Bits`] impl, a field
/// type needs the same conversions reachable as inherent `const fn`s. This macro
/// emits **both from one definition**: the trait impl delegates to the inherent
/// pair, so the two can never disagree, and the pair's naming stays an
/// implementation detail of the crate.
///
/// The bodies must satisfy the [`Bits`] contract: `into_bits` yields the value in
/// the low `BITS` bits of a `u128`, `from_bits` reconstructs from the low `BITS`
/// bits (higher bits ignored), and the two round-trip.
///
/// ```
/// use bnb::{bitfield, u7};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// struct Percent(u7);
///
/// bnb::impl_bits! {
///     impl Bits for Percent {
///         const BITS: u32 = 7;
///         const fn into_bits(self) -> u128 { self.0.value() as u128 }
///         const fn from_bits(raw: u128) -> Self { Percent(u7::from_raw(raw as u8)) }
///     }
/// }
///
/// // `Percent` now nests as a `#[bitfield]` field like any built-in `Bits` type.
/// #[bitfield(u8, bits = msb)]
/// #[derive(Clone, Copy)]
/// struct Meter { pct: Percent, on: bool }
///
/// let m = Meter::new().with_pct(Percent(u7::new(42))).with_on(true);
/// assert_eq!(m.pct(), Percent(u7::new(42)));
/// assert!(m.on());
/// ```
#[macro_export]
macro_rules! impl_bits {
    (
        impl Bits for $t:ty {
            const BITS: u32 = $bits:expr;
            const fn into_bits($self_:ident) -> u128 $into:block
            const fn from_bits($raw:ident: u128) -> Self $from:block
        }
    ) => {
        impl $t {
            // The const-dispatch pair `#[bitfield]` accessors call (see `Bits`'
            // docs). `pub` because a bitfield in any other module/crate must be
            // able to reach it; `allow(unreachable_pub)` for crate-private types.
            #[doc(hidden)]
            #[allow(unreachable_pub)]
            #[inline]
            pub const fn __bnb_into_bits($self_) -> u128 $into

            #[doc(hidden)]
            #[allow(unreachable_pub)]
            #[inline]
            pub const fn __bnb_from_bits($raw: u128) -> Self $from
        }

        impl $crate::Bits for $t {
            const BITS: u32 = $bits;

            #[inline]
            fn into_bits(self) -> u128 {
                self.__bnb_into_bits()
            }

            #[inline]
            fn from_bits(raw: u128) -> Self {
                Self::__bnb_from_bits(raw)
            }
        }
    };
}

/// The seam every `#[bitfield]` struct implements — the stable interface the
/// `#[bin]` codec builds on, independent of how the fields are accessed.
///
/// A bitfield is a thin wrapper over a single backing unsigned integer; this
/// trait exposes that backing plus the declared layout metadata. The generated
/// type also provides allocation-free byte conversions: inherent `to_bytes`/`from_bytes`
/// (which use the declared [`BYTE_ORDER`](Bitfield::BYTE_ORDER)) plus the
/// endianness-explicit `to_be_bytes`/`to_le_bytes`/`from_be_bytes`/`from_le_bytes`.
///
/// # Examples
///
/// ```
/// use bnb::{bitfield, u4, Bitfield, BitOrder, ByteOrder};
///
/// #[bitfield(u8, bits = msb, bytes = big)]
/// #[derive(Clone, Copy)]
/// struct Byte { hi: u4, lo: u4 }
///
/// let b = Byte::new().with_hi(u4::new(0xA)).with_lo(u4::new(0xB));
/// assert_eq!(b.to_raw(), 0xAB);              // the backing integer
/// assert_eq!(Byte::from_raw(0xCD).hi().value(), 0xC);
/// assert_eq!(Byte::WIDTH, 8);                // declared layout metadata
/// assert_eq!(Byte::BYTE_ORDER, ByteOrder::Big);
/// assert_eq!(Byte::BIT_ORDER, BitOrder::Msb);
/// ```
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

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Nibble(u8);

    crate::impl_bits! {
        impl Bits for Nibble {
            const BITS: u32 = 4;
            const fn into_bits(self) -> u128 {
                self.0 as u128
            }
            const fn from_bits(raw: u128) -> Self {
                Nibble((raw & 0xF) as u8)
            }
        }
    }

    #[test]
    fn impl_bits_emits_a_delegating_trait_impl_and_a_const_pair() {
        assert_eq!(<Nibble as Bits>::BITS, 4);
        // The trait path routes through the user's bodies (masking preserved).
        assert_eq!(Nibble::from_bits(0xAB), Nibble(0xB));
        assert_eq!(Nibble(0x7).into_bits(), 0x7);
        // The inherent pair is `const` — usable where the accessors need it.
        const N: Nibble = Nibble::__bnb_from_bits(0x1C);
        assert_eq!(N, Nibble(0xC));
        const _: () = assert!(Nibble(0x3).__bnb_into_bits() == 0x3);
    }
}
