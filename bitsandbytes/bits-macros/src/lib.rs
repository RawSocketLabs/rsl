//! Procedural macros for the [`bits`](https://docs.rs/bits) crate.
//!
//! - [`macro@bitfield`] — pack typed fields into one backing integer with
//!   explicit bit and byte order.
//! - [`macro@BitEnum`] — derive enum ⇄ integer with an optional catch-all.
//!
//! These are re-exported from `bits`; depend on that crate, not this one.

#![deny(missing_docs)]

mod bitenum;
mod bitfield;
mod bitflags;
mod builder;
#[cfg(feature = "binrw")]
mod wire;

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
/// `| & ^ - !` (+ assign) operators; and `Bits`/`Bitfield` (+ binrw) impls so a
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
#[proc_macro_derive(BitEnum, attributes(bit_enum, catch_all))]
pub fn bit_enum(item: TokenStream) -> TokenStream {
    bitenum::expand(item)
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
/// field, and `build() -> Result<Foo, bits::BuilderError>` that errors on the
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

/// Folds a protocol message's binrw codec, builder, collapsed bit-groups,
/// derived fields, and soundness check into one attribute.
///
/// `#[wire]` is *sugar over the existing primitives*: it rewrites the struct
/// into a `#[binrw]` struct — so **every** binrw attribute (`magic`, `count`,
/// `args`, `import`, `map`, `parse_with`, `if`, `pre_assert`, `pad_*`, …) stays
/// usable as an escape hatch — and additionally generates a private `#[bitfield]`
/// per bit-group and a [`BitsBuilder`](macro@BitsBuilder)-style builder.
///
/// Requires the `binrw` feature (it wraps binrw) and that the dependent crate has
/// `binrw` as a direct dependency.
///
/// ```ignore
/// #[wire(big, group(opcode, flags, rcode => u16), validate = Header::soundness)]
/// #[derive(Debug, Clone, PartialEq)]
/// struct Header {
///     id: u16,
///     opcode: OpCode,           // these three are packed into one u16 on the
///     flags:  Flags,            // wire (a private #[bitfield]) but stay
///     rcode:  RCode,            // first-class in the builder and as fields
///     #[update(self.queries.len() as u16)]
///     qdcount: u16,             // derived on write, temp on read (not stored)
///     #[br(count = qdcount)]    // escape hatch: a raw binrw attribute
///     #[builder(default)]
///     queries: Vec<Question>,
/// }
/// ```
///
/// ## Attribute arguments
///
/// - `big` | `little` (default `big`): wire byte order (`#[brw(big|little)]`).
/// - `group(a, b, c => uN)` (repeatable): pack the **named, consecutive,
///   in-order** fields into a `uN` word. Naming the fields means a moved or
///   renamed field is a compile error; the macro also rejects members that are
///   not adjacent or are out of order.
/// - `validate = path`: `path: fn(&Self) -> Result<(), E: Display>`. Auto-creates
///   a `check_soundness` builder-only flag (default `true`); `build()` runs the
///   validator when it is set, and a `validate(&self)` method is generated for
///   opt-in post-parse checking. The **parser stays permissive** — it never
///   rejects representable input (the workspace's dual-use rule).
/// - `no_builder`: skip builder generation (codec + groups only).
///
/// ## Field attributes
///
/// - `#[update(expr)]`: derived field — `expr` is written every time and the
///   field is read into a temp (not stored, not in the builder).
/// - `#[builder_only]` / `#[builder_only(default = expr)]`: a field in the struct
///   and builder but **not** on the wire.
/// - `#[builder(default)]` / `#[builder(default = expr)]`: builder default policy
///   (as for [`BitsBuilder`](macro@BitsBuilder)).
/// - any `#[br]` / `#[bw]` / `#[brw]`: forwarded verbatim to binrw.
#[cfg(feature = "binrw")]
#[proc_macro_attribute]
pub fn wire(attr: TokenStream, item: TokenStream) -> TokenStream {
    wire::expand(attr, item)
}
