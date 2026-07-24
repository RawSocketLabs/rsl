//! Procedural macros for the [`bnb`](https://docs.rs/bnb) crate.
//!
//! - [`macro@bitfield`] — pack typed fields into one backing integer with
//!   explicit bit and byte order.
//! - [`macro@bitflags`] — pack named single-bit flags into one integer, with set
//!   algebra.
//! - [`macro@BitEnum`] — derive enum ⇄ integer with an optional catch-all.
//! - [`macro@BitDecode`] / [`macro@BitEncode`] — the low-level read/write codec
//!   derives (fields at arbitrary bit offsets).
//! - [`macro@bin`] — the unified whole-message codec attribute, folding the codec
//!   derives plus a builder (also dispatches tagged-union enums).
//! - [`macro@BitsBuilder`] — a required-by-default builder.
//!
//! These are re-exported from `bnb`; depend on that crate, not this one.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bitenum;
mod bitfield;
mod bitflags;
mod bitstream;
mod builder;

use proc_macro::TokenStream;

/// The path to the runtime crate as the crate currently being compiled refers to
/// it, for generated code to reference.
///
/// The runtime is **published as `bitsandbytes`** but its **library is named `bnb`**
/// (`[lib] name`); downstream consumers import it as `bnb` via
/// `bnb = { package = "bitsandbytes" }`. Cargo links the library by its lib name
/// (`bnb`) for any *non-renamed* reference — the crate's own tests/doctests/examples,
/// `trybuild`'s temp crates, and an un-aliased `bitsandbytes = "…"` consumer — and by
/// the *dependency key* for a `package = "…"`-renamed one. So:
///
/// - [`proc_macro_crate`] reporting the package name `bitsandbytes` (or `Itself`, or
///   not found) ⇒ a non-renamed reference ⇒ emit the lib name `::bnb`. `lib.rs` carries
///   `extern crate self as bnb;` so `::bnb` resolves inside the lib itself too.
/// - any other name ⇒ a `package = "…"` rename ⇒ emit that key (`::#name`).
pub(crate) fn bnb_path() -> proc_macro2::TokenStream {
    use proc_macro_crate::{FoundCrate, crate_name};
    match crate_name("bitsandbytes") {
        // Renamed dependency: the key is the import name.
        Ok(FoundCrate::Name(name)) if name != "bitsandbytes" => {
            let ident = proc_macro2::Ident::new(&name, proc_macro2::Span::call_site());
            quote::quote!(::#ident)
        }
        // Non-renamed (`Name("bitsandbytes")`), the crate itself, or unresolved: all
        // link the library by its lib name `bnb`.
        _ => quote::quote!(::bnb),
    }
}

/// The `BitDecode`/`BitEncode` impls a `Bits` type carries so it's usable as a `#[bin]` field
/// without a `#[nested]` marker — thin delegations to the bit read/write. Emitted by every
/// `Bits`-producing macro (`#[bitfield]`, `#[derive(BitEnum)]`, `#[bitflags]`) for the type it
/// generates; coheres because each is a concrete impl (no blanket).
pub(crate) fn bits_leaf_codec_impl(
    name: &syn::Ident,
    bnb: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    quote::quote! {
        impl #bnb::__private::BitDecode for #name {
            #[inline]
            fn bit_decode<__S: #bnb::__private::Source>(
                __r: &mut __S,
            ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                __r.read::<Self>()
            }
        }
        impl #bnb::__private::BitEncode for #name {
            #[inline]
            fn bit_encode<__K: #bnb::__private::Sink>(
                &self,
                __w: &mut __K,
            ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                __w.write(*self)
            }
        }
        impl #bnb::__private::FixedBitLen for #name {
            const BIT_LEN: u32 = <Self as #bnb::__private::Bits>::BITS;
        }
    }
}

/// Whether `ty` is exactly the bare single-segment path `name` (no qualifier, no
/// generics) — the only shape the const-dispatch helpers may convert inline.
fn is_bare_ident(ty: &syn::Type, name: &str) -> bool {
    if let syn::Type::Path(p) = ty {
        p.qself.is_none()
            && p.path.leading_colon.is_none()
            && p.path.segments.len() == 1
            && p.path.segments[0].arguments.is_none()
            && p.path.segments[0].ident == name
    } else {
        false
    }
}

/// The `Bits::from_bits` contract as a `const`-evaluable expression over `#raw: u128`.
///
/// A `const fn` cannot call trait methods on stable Rust, so generated accessors
/// dispatch by type instead of through `Bits`: `bool` and the primitive unsigned
/// integers convert inline, everything else through the hidden inherent
/// `__bnb_from_bits` pair that every `Bits`-producing macro (and `UInt`) provides.
/// The trait impls delegate to the same pair, so the two can never disagree.
pub(crate) fn const_from_bits(
    ty: &syn::Type,
    raw: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    if is_bare_ident(ty, "bool") {
        return quote::quote!(((#raw & 1) != 0));
    }
    for prim in ["u8", "u16", "u32", "u64", "u128"] {
        if is_bare_ident(ty, prim) {
            return quote::quote!((#raw as #ty));
        }
    }
    quote::quote!(<#ty>::__bnb_from_bits(#raw))
}

/// The `Bits::into_bits` contract as a `const`-evaluable expression over a `#value`
/// of type `ty` — the write-side dual of [`const_from_bits`].
pub(crate) fn const_into_bits(
    ty: &syn::Type,
    value: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    if is_bare_ident(ty, "bool") {
        return quote::quote!((#value as u128));
    }
    for prim in ["u8", "u16", "u32", "u64", "u128"] {
        if is_bare_ident(ty, prim) {
            return quote::quote!((#value as u128));
        }
    }
    quote::quote!(<#ty>::__bnb_into_bits(#value))
}

/// Packs the annotated struct's fields into a single backing integer.
///
/// ```ignore
/// #[bitfield(u16, bits = msb, bytes = big)]
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
/// - `bytes = big | little` (default `big`): byte order of the backing integer when
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
/// `new()` (all-zero; derive `Default` yourself if you want it),
/// `with_<field>`/`set_<field>`, `<field>()`
/// getters, `to_raw()`/`from_raw()`, `to_be_bytes()`/`to_le_bytes()`/
/// `from_be_bytes()`/`from_le_bytes()`, and `bnb::{Bits, Bitfield}` impls.
///
/// Every accessor is a **`const fn`** (`#[view]` accessors too, when their raw
/// type is annotated — see `bnb::guide::bitfields`; a custom field type needs
/// the inherent conversion pair documented on `bnb::Bits`).
///
/// ## Layout
///
/// The emitted struct is **`#[repr(transparent)]`** over its backing integer:
/// its size, alignment, and ABI are exactly the backing type's. (`transparent`,
/// not `repr(C)`, because the struct wraps a single native integer — that is
/// the honest guarantee; before 0.3.2 the layout was formally unspecified.)
/// Writing your own `#[repr(...)]` on the struct suppresses the generated one
/// — e.g. `repr(C)`, or `repr(align(N))`, neither of which can combine with
/// `transparent`.
///
/// See the `bnb::guide::bitfields` page for runnable examples (bit/byte order,
/// inferred vs. ranged widths, nesting).
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
/// - `bytes = big | little` (default `big`): byte order when serialized.
///
/// ## Generated API
///
/// A `const` per flag (upper-cased: `fin` → `TcpFlags::FIN`); `empty()`/`all()`/
/// `bits()`/`from_bits` (retains unknown bits) / `from_bits_truncate`;
/// `contains`/`intersects`/`is_empty`/`insert`/`remove`/`toggle`/`set`;
/// `union`/`intersection`/`difference`/`complement` (for combination
/// consts); per-flag `fin()`/`with_fin(bool)`/`set_fin(bool)`; `iter()`; the
/// `| & ^ - !` (+ assign) operators; and `Bits`/`Bitfield` impls so a
/// flag set nests in a `#[bitfield]` and serializes. Everything except `iter()`
/// and the operator impls is a **`const fn`**, and the emitted struct is
/// **`#[repr(transparent)]`** over its backing (suppressed by writing your own
/// `#[repr(...)]`), as for `#[bitfield]`.
///
/// See the `bnb::guide::flags` page for runnable examples (set algebra, iteration,
/// retain-vs-truncate, nesting).
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
/// (holding a `uN`/integer) may capture unknown discriminants. Without a catch-all
/// the variants must cover the whole width, or the enum must be declared
/// `#[bit_enum(uN, closed)]` to assert a closed set — otherwise it is a **compile
/// error**, because `from_bits` (the infallible codec / `#[bitfield]`-getter path)
/// would panic on an unknown discriminant. A `closed` enum still `unreachable!`s on
/// that path; its checked `TryFrom` rejects unknowns instead.
///
/// ## Generated API
///
/// Always: the `bnb::{Bits, BitEnum}` impls (so the enum nests in a
/// `#[bitfield]`). For a **primitive** width (`u8`/`u16`/`u32`/`u64`/`u128`)
/// additionally — for `num_enum` parity:
///
/// - `From<Enum> for uN` (every variant maps to a value);
/// - with `#[catch_all]`: `From<uN> for Enum` (total — unknowns absorbed);
/// - without it: `TryFrom<uN> for Enum`, erroring with `bnb::UnknownDiscriminant` on an
///   unknown value.
///
/// A non-primitive width (`u4`, or even a byte-aligned `u24`) gets none of these — such an
/// enum is only meaningful nested in a `#[bitfield]`.
///
/// See the `bnb::guide::enums` page for runnable examples (catch-all, `closed`, the
/// `num_enum` parity, and nesting).
#[proc_macro_derive(BitEnum, attributes(bit_enum, catch_all))]
pub fn bit_enum(item: TokenStream) -> TokenStream {
    bitenum::expand(item)
}

/// Derives a `bnb::BitDecode` impl that reads the struct's named fields, in
/// declaration order, from a **bit** cursor (a `bnb::BitReader`).
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
/// Each leaf field is read with `Source::read`, so any `bnb::Bits` type
/// (`u1`..`u127`, `#[bitfield]`, `#[derive(BitEnum)]`) works as a field — no
/// byte-alignment, seeks, or shift glue. A field marked **`#[nested]`** is itself
/// a `BitDecode`/`BitEncode` message and is recursed into (a fixed one's
/// `FixedBitLen::BIT_LEN` counts toward the parent's). `[u8; N]` payloads and
/// `#[br(count = …)]` `Vec`s are supported; a `count`-bearing message is
/// variable-length and so does not implement `bnb::FixedBitLen`.
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
///
/// See the `bnb::guide::directives` page for a runnable example of each `#[br]`/`#[bw]`
/// directive, and `bnb::guide::bin_codec` for the `#[bin]` front-end.
#[proc_macro_derive(
    BitDecode,
    attributes(bit_stream, nested, br, bw, brw, reserved, reserved_with)
)]
pub fn bit_decode(item: TokenStream) -> TokenStream {
    bitstream::expand_decode(item)
}

/// Derives a `bnb::BitEncode` impl — the dual of [`macro@BitDecode`], writing
/// the struct's named fields in order to a `bnb::BitWriter` bit cursor. Shares
/// [`BitDecode`](macro@BitDecode)'s right-tool guard and `#[bit_stream(...)]`
/// override.
#[proc_macro_derive(
    BitEncode,
    attributes(bit_stream, nested, br, bw, brw, reserved, reserved_with)
)]
pub fn bit_encode(item: TokenStream) -> TokenStream {
    bitstream::expand_encode(item)
}

/// `#[bin]` — the unified whole-message bit codec. One attribute that folds the read
/// codec ([`BitDecode`](macro@BitDecode)), the write codec
/// ([`BitEncode`](macro@BitEncode)), and a required-by-default builder
/// ([`BitsBuilder`](macro@BitsBuilder)) over a struct of `bnb::Bits` fields, read and
/// written at arbitrary bit offsets.
///
/// ```ignore
/// #[bin(big, validate = Frame::check)]
/// #[derive(Debug, PartialEq)]
/// struct Frame {
///     version: u4,
///     #[builder(default)] flags: u4,
///     #[br(temp)] #[bw(calc = self.payload.len() as u16)] len: u16,
///     #[br(count = len)] payload: Vec<u8>,
/// }
/// // -> Frame::{decode, decode_all, decode_iter, peek, decode_exact},
/// //    Frame::{to_bytes, encode}, Frame::new(..), and Frame::builder()
/// ```
///
/// ## Struct-level options
///
/// `big` / `little` or `bytes = big|little` (byte order), `bits = msb|lsb`, `magic = <expr>` (a leading
/// constant verified on read / emitted on write), `read_only` / `write_only`
/// (directional), `no_builder`, `forward_only` (bound decoding to a forward `Source`),
/// `ctx(name: Ty, …)` (context from the parent), `validate = <path>` (a soundness
/// check run by `build()` — the parser stays permissive),
/// `map`/`try_map`/`wire`/`try_wire` (map the whole struct to/from a wire type — see
/// `bnb::guide::mapping`), `auto_len(<field>.<nested> = count|bytes(<source>), …)`
/// (cross-struct `WireLen` derivation), and — on a **single-field
/// tuple newtype** — `codec = <module>` / `codec(parse = <f>, write = <f>)` (the
/// per-type field codec: the type's wire form is owned by the fn pair; use it as a
/// plain field anywhere, with `#[brw(variable)]` in a fixed parent).
///
/// `bits = msb` and `bytes = big` are the defaults, and `big`+`msb` is the natural
/// pairing (network byte order); the declared byte order only swaps a **byte-multiple**
/// value that *differs* from the bit order's natural layout, never a sub-byte field. See
/// `bnb::guide::bin_codec` § "Byte order × bit order" for the rule.
///
/// ## Field directives
///
/// `#[br]`/`#[bw]`: `count`, `ctx { … }`, `temp` + `calc`, `if(…)`,
/// `assert(<expr>[, "fmt", args…])` (a decode-time guard over this and earlier fields —
/// the explicit opt-in strictness escape hatch; read-only, no inverse), `map`/`try_map`
/// (+ the inverse `bw(map)`), `parse_with`/`write_with`, `pad_before/after`,
/// `align_before/after`, `seek = <bits>`, `restore_position`, `dbg` (trace a field as it
/// decodes), `#[bw(auto_len = count(<field>)|bytes(<field>))]` (derive a `WireLen<T>`
/// field from a sibling target — the primary `WireLen` workflow); `#[brw(ignore)]`
/// (neither read nor written); `#[brw(variable)]` (the
/// field's type is a variable-length custom codec — suppresses the parent's
/// `FixedBitLen`); `#[brw(count_prefix = <Ty>)]`
/// on a `Vec<_>` (the length-prefixed count sugar — generates the `temp`+`calc`+`count`
/// triad: the prefix sizes the `Vec` on read and is recomputed, **checked**, from
/// `len()` on write; any `Bits` prefix type incl. `uN`); `#[try_str]` (render a
/// byte-buffer field as text in `Debug`); plus `#[reserved]` / `#[reserved_with(…)]`.
///
/// ## On an enum — tagged-union dispatch
///
/// `#[bin]` also applies to an enum, dispatching on two orthogonal concepts: a
/// **`magic`** wire constant (a byte string or width-suffixed unsigned int, read *and*
/// written — the discriminant, or a verified signature on a tag-variant) and a **`tag`**
/// selector taken from `ctx` (read-only, never on the wire). With per-variant `magic`s
/// the enum reads the discriminant and matches (a single `==` for uniform widths, or a
/// peek-and-`starts_with` for variable-width byte strings). With `#[bin(tag = <ctx-param>)]`
/// and variant `#[bin(tag = V)]` it dispatches on the selector and writes no discriminant;
/// the two compose (and may even be mixed in one enum — tag priority, then magic). The
/// "nothing matched" tail is a `#[catch_all]` (preserving the unknown discriminant) or a
/// typed no-tag/no-magic fallback variant, not both; without either a magic enum is a
/// closed set. An optional enum-level `magic` is a leading prefix. Variants may be unit,
/// tuple, named, or `#[nested]`. The codec also generates `decode_as_<variant>`,
/// `peek_variant`/`<Name>Kind`, `decode_tagged`, and a `magic()`/`tag()` accessor where
/// the discriminant is single-valued. See the `bnb::guide::dispatch` page.
///
/// On a struct, `#[bin]` lowers to `#[derive(BitDecode, BitEncode, BitsBuilder)]`, which
/// stay usable directly. See the `bnb::guide::bin_codec` page for a full walkthrough and
/// `bnb::guide::directives` for one runnable example per directive.
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
///
/// See the `bnb::guide::builders` page for runnable examples (required-field errors,
/// `default`/`default = expr`, the `#[bitfield]` intercept).
#[proc_macro_derive(BitsBuilder, attributes(builder))]
pub fn bits_builder(item: TokenStream) -> TokenStream {
    builder::expand_derive(item)
}
