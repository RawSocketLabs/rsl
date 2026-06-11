//! Procedural macros for the [`bits`](https://docs.rs/bits) crate.
//!
//! - [`macro@bitfield`] — pack typed fields into one backing integer with
//!   explicit bit and byte order.
//! - [`macro@BitEnum`] — derive enum ⇄ integer with an optional catch-all.
//!
//! These are re-exported from `bits`; depend on that crate, not this one.

mod bitenum;
mod bitfield;

use proc_macro::TokenStream;

/// Packs the annotated struct's fields into a single backing integer.
///
/// ```ignore
/// #[bitfield(u16, bits = msb, bytes = be)]
/// #[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// struct State {
///     opcode: u5,    // first field -> high bits (msb)
///     flags:  Flags, // a nested bitfield
///     rcode:  RCode, // a BitEnum; last field -> low bits
/// }
/// ```
///
/// ## Attribute arguments
///
/// - **backing** (first, required): the storage primitive — `u8`, `u16`, `u32`,
///   `u64`, or `u128`. Must be at least as wide as the fields.
/// - `bits = msb | lsb` (default `msb`): whether the first declared field lands
///   in the high or low bits.
/// - `bytes = be | le` (default `be`): byte order of the backing integer when
///   serialized.
///
/// ## Field widths
///
/// A field's width is, in order of precedence: an explicit `#[bits(N)]`; an
/// explicit `#[bits(A..=B)]` range (which also fixes its absolute offset); or,
/// by default, `<FieldType as bits::Bits>::BITS`. Use widths/inference for
/// automatic layout, or ranges on **every** field for fully manual layout — the
/// two styles cannot be mixed in one struct.
///
/// ## Generated API
///
/// `new()`/`Default` (all-zero), `with_<field>`/`set_<field>`, `<field>()`
/// getters, `raw()`/`from_raw()`, `to_be_bytes()`/`to_le_bytes()`/
/// `from_be_bytes()`/`from_le_bytes()`, and `bits::{Bits, Bitfield}` impls. With
/// the `binrw` feature, also `BinRead`/`BinWrite` using the declared byte order.
#[proc_macro_attribute]
pub fn bitfield(attr: TokenStream, item: TokenStream) -> TokenStream {
    bitfield::expand(attr, item)
}

/// Derives an enum ⇄ integer mapping of a fixed bit width.
///
/// ```ignore
/// #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
/// #[bit_enum(u4)]
/// enum RCode {
///     NoError = 0,
///     FormErr = 1,
///     #[catch_all]
///     Other(u4),   // preserves unknown values (dual-use)
/// }
/// ```
///
/// `#[bit_enum(uN)]` sets the width. Exactly one `#[catch_all]` tuple variant
/// (holding a `uN`/integer) may capture unknown discriminants; without one, an
/// unknown value decodes to [`bits::Error::UnknownVariant`].
#[proc_macro_derive(BitEnum, attributes(bit_enum, catch_all))]
pub fn bit_enum(item: TokenStream) -> TokenStream {
    bitenum::expand(item)
}
