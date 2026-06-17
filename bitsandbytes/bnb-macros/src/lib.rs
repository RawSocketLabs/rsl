//! Procedural macros for the [`bnb`](https://docs.rs/bnb) crate.
//!
//! - [`macro@bitfield`] — pack typed fields into one backing integer with
//!   explicit bit and byte order.
//! - [`macro@BitEnum`] — derive enum ⇄ integer with an optional catch-all.
//!
//! These are re-exported from `bnb`; depend on that crate, not this one.

#![deny(missing_docs)]

mod bitenum;
mod bitfield;
mod bitflags;
mod bitstream;
mod builder;

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
/// by default, `<FieldType as bnb::Bits>::BITS`. Use widths/inference for
/// automatic layout, or ranges on **every** field for fully manual layout — the
/// two styles cannot be mixed in one struct.
///
/// ## Generated API
///
/// `new()`/`Default` (all-zero), `with_<field>`/`set_<field>`, `<field>()`
/// getters, `raw()`/`from_raw()`, `to_be_bytes()`/`to_le_bytes()`/
/// `from_be_bytes()`/`from_le_bytes()`, and `bnb::{Bits, Bitfield}` impls.
#[proc_macro_attribute]
pub fn bitfield(attr: TokenStream, item: TokenStream) -> TokenStream {
    bitfield::expand(attr, item)
}

/// Packs named single-bit flags into one backing integer, with set algebra.
///
/// ```ignore
/// #[bitflags(u8)]
/// #[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// struct TcpFlags {
///     fin: bool,   // bit 0 (auto, LSB-first)
///     syn: bool,   // bit 1
///     #[flag(5)] ack: bool, // pinned to bit 5
/// }
/// ```
///
/// ## Attribute arguments
///
/// - **backing** (first, required): `u8`/`u16`/`u32`/`u64`/`u128`.
/// - `bytes = be | le` (default `be`): byte order when serialized.
///
/// ## Generated API
///
/// A `const` per flag (upper-cased: `fin` → `TcpFlags::FIN`); `empty()`/`all()`/
/// `bits()`/`from_bits` (retains unknown bits) / `from_bits_truncate`;
/// `contains`/`intersects`/`is_empty`/`insert`/`remove`/`toggle`/`set`;
/// const `union`/`intersection`/`difference`/`complement` (for combination
/// consts); per-flag `fin()`/`with_fin(bool)`/`set_fin(bool)`; `iter()`; the
/// `| & ^ - !` (+ assign) operators; and `Bits`/`Bitfield` impls so a
/// flag set nests in a `#[bitfield]` and serializes.
#[proc_macro_attribute]
pub fn bitflags(attr: TokenStream, item: TokenStream) -> TokenStream {
    bitflags::expand(attr, item)
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
/// unknown value triggers an `unreachable!` (the enum is assumed exhaustive).
///
/// ## Generated API
///
/// Always: the `bnb::{Bits, BitEnum}` impls (so the enum nests in a
/// `#[bitfield]`). For a **byte-aligned** width (`u8`/`u16`/…) additionally —
/// for `num_enum` parity:
///
/// - `From<Enum> for uN` (every variant maps to a value);
/// - with `#[catch_all]`: `From<uN> for Enum` (total — unknowns absorbed);
/// - without it: `TryFrom<uN> for Enum`, erroring with
///   [`bnb::UnknownDiscriminant`](bnb::UnknownDiscriminant) on an
///   unknown value.
///
/// A sub-byte enum (`u4`) gets none of these — it is only meaningful nested in a
/// `#[bitfield]`.
#[proc_macro_derive(BitEnum, attributes(bit_enum, catch_all))]
pub fn bit_enum(item: TokenStream) -> TokenStream {
    bitenum::expand(item)
}

/// Derives a [`bnb::BitDecode`] impl that reads the struct's named fields, in
/// declaration order, from a **bit** cursor ([`bnb::BitReader`]).
///
/// ```ignore
/// #[derive(BitDecode, BitEncode, Copy, Clone, Debug, PartialEq, Eq)]
/// struct GenericBurst {   // a 264-bit DMR burst, fields at bit offsets
///     p1: u108,           // bits   0..108
///     pattern: SyncPattern,// bits 108..156 (a 48-bit #[derive(BitEnum)])
///     p2: u108,           // bits 156..264
/// }
/// ```
///
/// Each leaf field is read with `Source::read`, so any [`bnb::Bits`] type
/// (`u1`..`u127`, `#[bitfield]`, `#[derive(BitEnum)]`) works as a field — no
/// byte-alignment, seeks, or shift glue. A field marked **`#[nested]`** is itself
/// a `BitDecode`/`BitEncode` message and is recursed into (a fixed one's
/// `FixedBitLen::BIT_LEN` counts toward the parent's). `[u8; N]` payloads and
/// `#[br(count = …)]` `Vec`s are supported; a `count`-bearing message is
/// variable-length and so does not implement [`bnb::FixedBitLen`].
///
/// ## Which codec? (the derive steers you)
///
/// | Your data | Use | Why |
/// |---|---|---|
/// | Fields straddle byte boundaries (e.g. `108 \| 48 \| 108` bits) | **`#[derive(BitDecode/BitEncode)]`** or **`#[bin]`** | reads at arbitrary bit offsets |
/// | A whole message (magic/counts/`Vec`/nesting/ctx/…), bit- or byte-aligned | **`#[bin]`** | the unified codec |
/// | A run of sub-byte fields that fills one integer | **`#[bitfield]`** | one packed word with named accessors |
///
/// To enforce that, the bare derive **rejects an all-byte-aligned struct** at
/// compile time (every field a whole number of bytes ⇒ the cursor never leaves byte
/// boundaries ⇒ `#[bin]` is the better tool). Override with the struct-level
/// `#[bit_stream(allow_byte_aligned)]` when you really mean it.
#[proc_macro_derive(
    BitDecode,
    attributes(bit_stream, nested, br, bw, brw, reserved, reserved_with)
)]
pub fn bit_decode(item: TokenStream) -> TokenStream {
    bitstream::expand_decode(item)
}

/// Derives a [`bnb::BitEncode`] impl — the dual of [`macro@BitDecode`], writing
/// the struct's named fields in order to a [`bnb::BitWriter`] bit cursor. Shares
/// [`BitDecode`](macro@BitDecode)'s right-tool guard and `#[bit_stream(...)]`
/// override.
#[proc_macro_derive(
    BitEncode,
    attributes(bit_stream, nested, br, bw, brw, reserved, reserved_with)
)]
pub fn bit_encode(item: TokenStream) -> TokenStream {
    bitstream::expand_encode(item)
}

/// `#[bin]` — the unified native bit-codec attribute (Phase 2). One macro that
/// folds the codec (`BitDecode`/`BitEncode`) and the required-by-default builder.
///
/// ```ignore
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Frame {
///     version: u4,
///     #[builder(default)] flags: u4,
///     payload_len: u12,
/// }
/// // -> Frame::decode/peek/decode_exact, encode/to_bytes, and Frame::builder()
/// ```
///
/// Struct-level options: `read_only` / `write_only` (directional), `no_builder`,
/// `bit_order = msb|lsb`, `allow_byte_aligned`. Field directives
/// (`#[br]`/`#[bw]`/`#[brw]`, …) arrive in later Phase 2 chunks. It lowers to
/// `#[derive(BitDecode, BitEncode, BitsBuilder)]`, which stay usable directly.
#[proc_macro_attribute]
pub fn bin(attr: TokenStream, item: TokenStream) -> TokenStream {
    bitstream::expand_bin(attr, item)
}

/// Generates a `derive_builder`-style builder for a struct (or, when listed in a
/// `#[bitfield]`'s derives, for the bitfield — intercepted by `#[bitfield]`).
///
/// ```ignore
/// #[bitfield(u16, bits = msb)]
/// #[derive(BitsBuilder, Clone, Copy)]
/// struct State {
///     opcode: u4,            // required
///     #[builder(default)]    // optional; 0 if unset
///     flags: u8,
///     rcode: RCode,          // required
/// }
///
/// let s = State::builder().opcode(u4::new(2)).rcode(RCode::ServFail).build()?;
/// ```
///
/// Generates `Foo::builder() -> FooBuilder`, an `Option`-tracked setter per
/// field, and `build() -> Result<Foo, bnb::BuilderError>` that errors on the
/// first unset **required** field. A field is optional only with
/// `#[builder(default)]` (`Default::default()` if unset) or
/// `#[builder(default = expr)]`. Coexists with the infallible infix `with_*`.
///
/// On a `#[bitfield]` struct, list `#[bitfield(...)]` **above** the `#[derive]`
/// so it intercepts this marker.
#[proc_macro_derive(BitsBuilder, attributes(builder))]
pub fn bits_builder(item: TokenStream) -> TokenStream {
    builder::expand_derive(item)
}
