//! Expansion of `#[derive(BitDecode)]` / `#[derive(BitEncode)]` — the bit-stream
//! message codec.
//!
//! Each generates an impl that reads/writes the struct's named fields **in
//! declaration order** from a `bnb::BitReader`/`BitWriter` bit cursor. A field
//! is read with `__bnb_r.read()` / written with `__bnb_w.write(self.field)`, which works for
//! any `bnb::Bits` type (`u1`..`u127`, `#[bitfield]`, `#[derive(BitEnum)]`), so
//! the bit-stream codec composes with the rest of the crate's macros. Nested
//! `#[nested]` messages, `[u8; N]` payloads, `magic`, `#[br(count = …)]` `Vec`s,
//! `ctx` parameterization, `#[br(temp)]`/`#[bw(calc = …)]`, `#[br(if(…))]`
//! conditional `Option`s, `#[br(map/try_map = …)]`/`#[bw(map = …)]` transforms, and
//! `#[reserved]`/`#[reserved_with(…)]` bits, positioning (`pad_*`/`align_*`/
//! `restore_position`), and the `parse_with`/`write_with` escape hatches are all
//! supported (`temp`/`calc`/`reserved` via `#[bin]`, which generates the codec
//! directly).
//!
//! ## Right-tool guard
//!
//! The bare `#[derive(BitDecode/BitEncode)]` is the low-level bit codec; if a
//! struct's fields are **all byte-aligned** (every width a multiple of 8) the cursor
//! never leaves byte boundaries, so `#[bin]` (the unified codec) is the better tool.
//! The derives emit a const-eval guard that rejects such a struct, steering the
//! author to `#[bin]`. The escape hatch is `#[bit_stream(allow_byte_aligned)]`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{
    Data, DeriveInput, Fields, FieldsNamed, Ident, ItemStruct, Token, Type, parse_macro_input,
};

/// A declared context parameter `name: Ty` from `ctx(name: Ty, …)`.
struct CtxParam {
    name: Ident,
    ty: Type,
}

impl Parse for CtxParam {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        Ok(CtxParam { name, ty })
    }
}

/// The generated context-struct name for a type/ident — `Foo` ⇒ `FooCtx`.
fn ctx_struct_ident(name: &Ident) -> Ident {
    format_ident!("{}Ctx", name)
}

/// The context-struct **type** for a field/element type — appends `Ctx` to the
/// last path segment so `m::Value` ⇒ `m::ValueCtx`.
fn ctx_struct_ty(ty: &Type) -> syn::Result<TokenStream2> {
    if let Type::Path(p) = ty {
        let mut path = p.path.clone();
        if let Some(last) = path.segments.last_mut() {
            last.ident = ctx_struct_ident(&last.ident);
            last.arguments = syn::PathArguments::None;
            return Ok(quote!(#path));
        }
    }
    Err(syn::Error::new_spanned(
        ty,
        "a `ctx`-parameterized field must have a path type (so its `…Ctx` struct can be named)",
    ))
}

/// The const-eval guard's message — steers the bare derive toward `#[bin]`.
const BYTE_ALIGNED_MSG: &str = "this struct's fields are all byte-aligned. The bare \
`#[derive(BitDecode/BitEncode)]` is the low-level bit codec; for a byte-aligned message use \
`#[bin]` — the unified codec (it handles byte-aligned data natively and adds \
magic/count/ctx/map/if/validate). The bare derive is for fields that straddle byte boundaries \
(e.g. a 108-bit payload). To keep the bare derive on an all-byte-aligned struct anyway, add \
`#[bit_stream(allow_byte_aligned)]`.";

/// Returns the named fields of a non-generic struct, or a well-spanned error.
fn named_struct(input: &DeriveInput) -> syn::Result<&FieldsNamed> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "BitDecode/BitEncode do not support generic parameters yet",
        ));
    }
    match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => Ok(f),
            _ => Err(syn::Error::new_spanned(
                &input.ident,
                "BitDecode/BitEncode require a struct with named fields",
            )),
        },
        _ => Err(syn::Error::new_spanned(
            &input.ident,
            "BitDecode/BitEncode can only derive for structs",
        )),
    }
}

/// Parsed struct-level `#[bit_stream(...)]` options.
#[derive(Default)]
struct BitStreamAttrs {
    /// `allow_byte_aligned` — opt out of the right-tool guard.
    allow_byte_aligned: bool,
    /// `bit_order = lsb` (else MSB-first, the default).
    lsb: bool,
    /// `little` / `byte_order = little` (else big-endian, the default).
    little: bool,
    /// `magic = <expr>` — a leading constant verified on read, emitted on write.
    /// Any `Bits` value, so it can even be sub-byte (`u3::new(0b110)`).
    magic: Option<syn::Expr>,
    /// `ctx(name: Ty, …)` — context this type needs from its parent. When present the
    /// type gets `decode_with`/`encode_with` (it does **not** implement
    /// `BitDecode`/`BitEncode`, which take no context).
    ctx: Vec<(Ident, Type)>,
}

fn parse_bit_stream(input: &DeriveInput) -> syn::Result<BitStreamAttrs> {
    let mut attrs = BitStreamAttrs::default();
    for attr in &input.attrs {
        if attr.path().is_ident("bit_stream") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("allow_byte_aligned") {
                    attrs.allow_byte_aligned = true;
                    Ok(())
                } else if meta.path.is_ident("bit_order") {
                    let val: Ident = meta.value()?.parse()?;
                    match val.to_string().as_str() {
                        "msb" => attrs.lsb = false,
                        "lsb" => attrs.lsb = true,
                        _ => return Err(meta.error("expected `msb` or `lsb`")),
                    }
                    Ok(())
                } else if meta.path.is_ident("byte_order") {
                    let val: Ident = meta.value()?.parse()?;
                    match val.to_string().as_str() {
                        "big" => attrs.little = false,
                        "little" => attrs.little = true,
                        _ => return Err(meta.error("expected `big` or `little`")),
                    }
                    Ok(())
                } else if meta.path.is_ident("magic") {
                    attrs.magic = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("ctx") {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let params = Punctuated::<CtxParam, Token![,]>::parse_terminated(&content)?;
                    attrs.ctx = params.into_iter().map(|p| (p.name, p.ty)).collect();
                    Ok(())
                } else {
                    Err(meta.error(
                        "unknown `#[bit_stream(...)]` option; expected `allow_byte_aligned`, `bit_order = msb|lsb`, `byte_order = big|little`, `magic = <expr>`, or `ctx(name: Ty, …)`",
                    ))
                }
            })?;
        }
    }
    Ok(attrs)
}

/// The runtime [`Layout`](bnb::Layout) (bit + byte order) for the struct.
fn layout_token(attrs: &BitStreamAttrs) -> TokenStream2 {
    let bnb = crate::bnb_path();
    let bit = if attrs.lsb {
        quote!(#bnb::__private::BitOrder::Lsb)
    } else {
        quote!(#bnb::__private::BitOrder::Msb)
    };
    let byte = if attrs.little {
        quote!(#bnb::__private::ByteOrder::Little)
    } else {
        quote!(#bnb::__private::ByteOrder::Big)
    };
    quote!(#bnb::__private::Layout { bit: #bit, byte: #byte })
}

/// Whether a field is a **nested message** (marked `#[nested]`) — a
/// `BitDecode`/`BitEncode` struct recursed into — rather than a `Bits` leaf.
/// An explicit marker: a nested message and a `Bits` leaf are both struct fields,
/// so the attribute disambiguates which codec path to emit.
fn is_nested(f: &syn::Field) -> bool {
    f.attrs.iter().any(|a| a.path().is_ident("nested"))
}

/// If the field is a fixed `[u8; N]` byte array, returns its length expression.
fn byte_array_len(f: &syn::Field) -> Option<&syn::Expr> {
    if let syn::Type::Array(arr) = &f.ty {
        if let syn::Type::Path(p) = &*arr.elem {
            if p.path.is_ident("u8") {
                return Some(&arr.len);
            }
        }
    }
    None
}

/// If the field's type is `Vec<T>`, returns the element type `T` — a
/// variable-length, `count`-driven field.
fn vec_elem(f: &syn::Field) -> Option<&syn::Type> {
    single_generic(&f.ty, "Vec")
}

/// If the field's type is `Option<T>`, returns the inner type `T` — a
/// conditional (`#[br(if(...))]`) field.
fn option_elem(f: &syn::Field) -> Option<&syn::Type> {
    single_generic(&f.ty, "Option")
}

/// The single type argument of `Wrapper<T>`, if `ty` is `Wrapper<T>`.
fn single_generic<'a>(ty: &'a syn::Type, wrapper: &str) -> Option<&'a syn::Type> {
    if let syn::Type::Path(p) = ty {
        let seg = p.path.segments.last()?;
        if seg.ident == wrapper {
            if let syn::PathArguments::AngleBracketed(a) = &seg.arguments {
                if let Some(syn::GenericArgument::Type(t)) = a.args.first() {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// Parsed field-level `#[br(...)]` directives.
#[derive(Default)]
struct FieldBr {
    /// `count = <expr>` — element count for a `Vec<T>` (may name an earlier field).
    count: Option<syn::Expr>,
    /// `ctx { a, b }` — pass context to a nested `ctx` message's `decode_with`/
    /// `encode_with`. Each name is a parent field or the parent's own ctx param.
    ctx: Option<Vec<Ident>>,
    /// `#[br(temp)]` — read into a local (usable by a later `count`/`ctx`) but do
    /// **not** store the field; `#[bin]` strips it from the struct. Pairs with
    /// `#[bw(calc = …)]` for the write side.
    temp: bool,
    /// `#[br(if(<expr>))]` — a conditional `Option<T>` field: read `Some` when the
    /// condition (over earlier fields, as locals) holds, else `None`; on encode the
    /// `Option`'s presence drives whether it is written.
    cond: Option<syn::Expr>,
    /// `#[brw(ignore)]` — a field that is **neither read nor written**: in-memory
    /// only, `Default::default()` on read (no input consumed) and skipped on write.
    /// Spelled with `brw` because it applies to both directions.
    ignore: bool,
    /// `#[br(map = <f>)]` — read the wire value `f`'s argument types, then `f(raw)`
    /// gives the field. `#[br(try_map = <f>)]` is the fallible form (`f` returns a
    /// `Result`); they are mutually exclusive.
    map: Option<syn::Expr>,
    try_map: Option<syn::Expr>,
    /// `#[br(parse_with = <f>)]` — the escape hatch: `f(r) -> Result<T, BitError>`
    /// reads the field with a custom function (`f: fn<S: Source>(&mut S) -> …`).
    parse_with: Option<syn::Expr>,
    /// `#[bw(calc = <expr>)]` — on encode, write `expr` (computed from the other
    /// fields) instead of `self.field`. The matched read/write pair is generated
    /// together so the directions can't drift.
    calc: Option<syn::Expr>,
    /// `#[bw(map = <f>)]` — on encode, write `f(&self.field)` (the wire value).
    bw_map: Option<syn::Expr>,
    /// `#[bw(write_with = <f>)]` — the escape hatch: `f(&self.field, w) -> Result<(),
    /// BitError>` writes the field with a custom function.
    write_with: Option<syn::Expr>,
    /// `#[br(pad_before/pad_after = <bits>)]` — skip a bit count around the field
    /// (`4.bits()` / `3.bytes()` via `bnb::prelude`). `align_before/align_after`
    /// skip to the next byte boundary.
    pad_before: Option<syn::Expr>,
    pad_after: Option<syn::Expr>,
    align_before: bool,
    align_after: bool,
    /// `#[br(restore_position)]` — read the field (a peek), then rewind the cursor so
    /// later fields re-read from the same offset; skipped on write. Seeks, so the
    /// generated `decode_from` is bound on [`SeekSource`](bnb::SeekSource): a
    /// forward-only stream is a compile error (the slice entry points
    /// `decode`/`peek`/`decode_exact` always qualify).
    restore_position: bool,
    /// `#[br(seek = <bits>)]` — before reading, jump the cursor to that **absolute**
    /// bit offset (e.g. following a pointer). A read-side primitive (the writer is
    /// append-only); pair with `restore_position` to read at an offset and return.
    /// Like `restore_position` it seeks, so `decode_from` is bound on
    /// [`SeekSource`](bnb::SeekSource). On encode the seek is a no-op — see the guide.
    seek: Option<syn::Expr>,
    /// `#[br(dbg)]` — emit a `tracing` event (TRACE level, target `bnb::dbg`) carrying
    /// the field's start offset and decoded value as it is read (the field type must be
    /// `Debug`). A read-only diagnostic: it consumes no extra bits and is inert on encode.
    dbg: bool,
}

/// One `#[br(...)]` directive. A hand-rolled parser (not `parse_nested_meta`)
/// because `if` is a keyword and can't be read as a meta path ident.
enum BrDirective {
    Count(syn::Expr),
    Ctx(Vec<Ident>),
    Temp,
    If(syn::Expr),
    Map(syn::Expr),
    TryMap(syn::Expr),
    ParseWith(syn::Expr),
    PadBefore(syn::Expr),
    PadAfter(syn::Expr),
    AlignBefore,
    AlignAfter,
    RestorePosition,
    Seek(syn::Expr),
    Dbg,
}

impl Parse for BrDirective {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![if]) {
            input.parse::<Token![if]>()?;
            let content;
            syn::parenthesized!(content in input);
            Ok(BrDirective::If(content.parse()?))
        } else {
            let kw: Ident = input.parse()?;
            match kw.to_string().as_str() {
                "count" => {
                    input.parse::<Token![=]>()?;
                    Ok(BrDirective::Count(input.parse()?))
                }
                "temp" => Ok(BrDirective::Temp),
                "ignore" => Err(syn::Error::new_spanned(
                    &kw,
                    "`ignore` marks a field as neither read nor written; write it as `#[brw(ignore)]`",
                )),
                "ctx" => {
                    let content;
                    syn::braced!(content in input);
                    let names = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
                    Ok(BrDirective::Ctx(names.into_iter().collect()))
                }
                "map" => {
                    input.parse::<Token![=]>()?;
                    Ok(BrDirective::Map(input.parse()?))
                }
                "try_map" => {
                    input.parse::<Token![=]>()?;
                    Ok(BrDirective::TryMap(input.parse()?))
                }
                "parse_with" => {
                    input.parse::<Token![=]>()?;
                    Ok(BrDirective::ParseWith(input.parse()?))
                }
                "pad_before" => {
                    input.parse::<Token![=]>()?;
                    Ok(BrDirective::PadBefore(input.parse()?))
                }
                "pad_after" => {
                    input.parse::<Token![=]>()?;
                    Ok(BrDirective::PadAfter(input.parse()?))
                }
                "align_before" => Ok(BrDirective::AlignBefore),
                "align_after" => Ok(BrDirective::AlignAfter),
                "restore_position" => Ok(BrDirective::RestorePosition),
                "seek" => {
                    input.parse::<Token![=]>()?;
                    Ok(BrDirective::Seek(input.parse()?))
                }
                "dbg" => Ok(BrDirective::Dbg),
                _ => Err(syn::Error::new_spanned(
                    kw,
                    "unknown `#[br(...)]` directive; expected `count`, `ctx`, `temp`, `if`, `map`, `try_map`, `parse_with`, `pad_before/after`, `align_before/after`, `restore_position`, `seek = <bits>`, or `dbg`",
                )),
            }
        }
    }
}

/// Parses a field's `#[br(count = …, ctx { … }, temp, if(…))]` and `#[bw(calc = …)]`.
fn parse_field_br(f: &syn::Field) -> syn::Result<FieldBr> {
    // The codec reads/writes through a generated source `__bnb_r` and sink `__bnb_w` — names
    // hygienic enough that a user field (even one named `r` or `w`) never shadows them, so
    // no field name is reserved.
    let mut br = FieldBr::default();
    for attr in &f.attrs {
        if attr.path().is_ident("br") {
            let directives =
                attr.parse_args_with(Punctuated::<BrDirective, Token![,]>::parse_terminated)?;
            for d in directives {
                match d {
                    BrDirective::Count(e) => br.count = Some(e),
                    BrDirective::Ctx(names) => br.ctx = Some(names),
                    BrDirective::Temp => br.temp = true,
                    BrDirective::If(e) => br.cond = Some(e),
                    BrDirective::Map(e) => br.map = Some(e),
                    BrDirective::TryMap(e) => br.try_map = Some(e),
                    BrDirective::ParseWith(e) => br.parse_with = Some(e),
                    BrDirective::PadBefore(e) => br.pad_before = Some(e),
                    BrDirective::PadAfter(e) => br.pad_after = Some(e),
                    BrDirective::AlignBefore => br.align_before = true,
                    BrDirective::AlignAfter => br.align_after = true,
                    BrDirective::RestorePosition => br.restore_position = true,
                    BrDirective::Seek(e) => br.seek = Some(e),
                    BrDirective::Dbg => br.dbg = true,
                }
            }
        } else if attr.path().is_ident("bw") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("calc") {
                    br.calc = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("map") {
                    br.bw_map = Some(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("write_with") {
                    br.write_with = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error(
                        "unknown `#[bw(...)]` directive; expected `calc = <expr>`, `map = <f>`, or `write_with = <f>`",
                    ))
                }
            })?;
        } else if attr.path().is_ident("brw") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("ignore") {
                    br.ignore = true;
                    Ok(())
                } else {
                    Err(meta.error("unknown `#[brw(...)]` directive; expected `ignore`"))
                }
            })?;
        }
    }
    if br.map.is_some() && br.try_map.is_some() {
        return Err(syn::Error::new_spanned(
            f,
            "`#[br(map = …)]` and `#[br(try_map = …)]` are mutually exclusive",
        ));
    }
    Ok(br)
}

/// Whether a field is `#[br(temp)]` (read into a local, not stored). Used by the
/// `#[bin]` front-end ([`bin_inner`]) to filter the emitted struct/builder; the codec
/// generators read `temp` off the pre-parsed [`FieldBr`] instead.
fn field_is_temp(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| br.temp)
}

/// Whether a field is `#[brw(ignore)]` (in-memory only — defaulted on read, not
/// written, zero wire bits). Read by [`field_width`], which has no parsed `br`.
fn field_is_ignore(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| br.ignore)
}

/// Whether a field's directives make the message variable-length / its width
/// indeterminate, so it is exempt from the alignment guard and the message never
/// implements `FixedBitLen`: a `ctx` child (not `Bits`/`FixedBitLen`), a conditional
/// `if` (present or absent), a custom codec (`map`/`try_map`/`parse_with`/`write_with`,
/// whose wire shape lives in the converter), or a positioning directive
/// (`pad_*`/`align_*`/`seek`/`restore_position`, which shifts the cursor).
fn br_indeterminate(br: &FieldBr) -> bool {
    br.ctx.is_some()
        || br.cond.is_some()
        || br.map.is_some()
        || br.try_map.is_some()
        || br.bw_map.is_some()
        || br.parse_with.is_some()
        || br.write_with.is_some()
        || br.pad_before.is_some()
        || br.pad_after.is_some()
        || br.align_before
        || br.align_after
        || br.restore_position
        || br.seek.is_some()
}

/// A reserved field — a normal stored field with a known **spec value** (the type's
/// zero, or the `reserved_with` expression). On the default codec path it reads/writes
/// like any field (so you observe and can override the actual wire bits); the canonical
/// encoder and the builder default use the spec value instead.
enum Reserved {
    /// `#[reserved]` — spec value is the type's zero.
    Zero,
    /// `#[reserved_with(<expr>)]` — spec value is `<expr>` (e.g. a must-be-one pattern).
    With(Box<syn::Expr>),
}

/// Parses a field's `#[reserved]` / `#[reserved_with(<expr>)]`, if present.
fn field_reserved(f: &syn::Field) -> syn::Result<Option<Reserved>> {
    for attr in &f.attrs {
        if attr.path().is_ident("reserved") {
            return Ok(Some(Reserved::Zero));
        }
        if attr.path().is_ident("reserved_with") {
            return Ok(Some(Reserved::With(Box::new(attr.parse_args()?))));
        }
    }
    Ok(None)
}

/// The spec value of a reserved field — what the canonical encoder writes and what the
/// builder defaults to: the type's zero for `#[reserved]`, the given expression for
/// `#[reserved_with(<expr>)]`. `None` if the field is not reserved. (On the verbatim
/// path a reserved field is a normal stored field; only the canonical encoder and the
/// builder default use this.)
fn reserved_spec_value(f: &syn::Field) -> syn::Result<Option<TokenStream2>> {
    let bnb = crate::bnb_path();
    let ty = &f.ty;
    Ok(field_reserved(f)?.map(|reserved| match reserved {
        Reserved::Zero => quote!(<#ty as #bnb::__private::Bits>::from_bits(0)),
        Reserved::With(expr) => {
            let expr = *expr;
            quote!({ let __r: #ty = #expr; __r })
        }
    }))
}

/// Whether a field carries `#[reserved]`/`#[reserved_with]` (a cheap attribute check).
fn field_is_reserved(f: &syn::Field) -> bool {
    f.attrs
        .iter()
        .any(|a| a.path().is_ident("reserved") || a.path().is_ident("reserved_with"))
}

/// Whether a message has a verbatim-vs-canonical distinction — a `reserved` field or a
/// non-`temp` `calc` field. Drives the canonical encoder *and* (for `#[bin]`) the in-memory
/// `encode_mode` field.
fn struct_has_canonical(fields: &FieldsNamed) -> bool {
    fields.named.iter().any(|f| {
        if field_is_reserved(f) {
            return true;
        }
        matches!(parse_field_br(f), Ok(br) if br.calc.is_some() && !br.temp)
    })
}

/// The derive partition for a `#[bin]` struct with an injected `encode_mode` field.
struct BinDerives {
    /// Derive paths re-emitted on the struct as-is (everything but the intercepted three).
    kept: Vec<syn::Path>,
    /// Non-`derive` outer attributes (doc comments, `#[repr]`, …), kept verbatim.
    others: Vec<syn::Attribute>,
    has_debug: bool,
    has_partial_eq: bool,
    has_hash: bool,
}

/// Splits a `#[bin]` struct's outer attributes, intercepting `Debug`/`PartialEq`/`Hash` —
/// `#[bin]` re-emits those as custom impls that exclude the injected `encode_mode` field.
/// `Eq` stays (it is a marker that holds with the extra field).
fn split_bin_derives(attrs: &[syn::Attribute]) -> syn::Result<BinDerives> {
    let mut kept = Vec::new();
    let mut others = Vec::new();
    let (mut has_debug, mut has_partial_eq, mut has_hash) = (false, false, false);
    for attr in attrs {
        if attr.path().is_ident("derive") {
            let paths =
                attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)?;
            for p in paths {
                if p.is_ident("Debug") {
                    has_debug = true;
                } else if p.is_ident("PartialEq") {
                    has_partial_eq = true;
                } else if p.is_ident("Hash") {
                    has_hash = true;
                } else {
                    kept.push(p);
                }
            }
        } else {
            others.push(attr.clone());
        }
    }
    Ok(BinDerives {
        kept,
        others,
        has_debug,
        has_partial_eq,
        has_hash,
    })
}

/// Whether a field attribute is one `#[bin]` consumes itself (`#[nested]`/`#[br]`/
/// `#[bw]` for the codec, `#[builder]` for the builder) and must strip from the
/// struct it emits — it generates the codec and builder directly, so nothing
/// registers these as helper attributes.
fn is_codec_field_attr(a: &syn::Attribute) -> bool {
    [
        "nested",
        "br",
        "bw",
        "brw",
        "builder",
        "reserved",
        "reserved_with",
    ]
    .iter()
    .any(|n| a.path().is_ident(n))
}

/// The context-struct literal for a `ctx { a, b }` pass, resolving each name:
/// on encode a parent **field** becomes `name: self.name`, anything else (the
/// parent's own ctx param, already a local) stays shorthand `name`. On decode all
/// names are locals, so all stay shorthand.
fn ctx_literal(
    ctx_ty: &TokenStream2,
    names: &[Ident],
    field_set: Option<&[&Ident]>,
) -> TokenStream2 {
    let inits = names.iter().map(|n| match field_set {
        Some(fields) if fields.contains(&n) => quote!(#n: self.#n),
        _ => quote!(#n),
    });
    quote!(#ctx_ty { #(#inits),* })
}

/// The bit-width expression for a field, used by the alignment guard (and, for a
/// fixed message, the `BIT_LEN` sum): a nested message contributes its
/// `FixedBitLen::BIT_LEN`, a fixed `[u8; N]` `N * 8`, a `Bits` leaf its `BITS`, a
/// `Vec<T>` its **element** width (its alignment is the element's). Resolved by
/// the compiler (the macro never computes widths).
fn field_width(f: &syn::Field) -> TokenStream2 {
    let bnb = crate::bnb_path();
    let ty = &f.ty;
    if field_is_ignore(f) {
        return quote!(0u32); // in-memory only: zero wire bits
    }
    if let Some(elem) = vec_elem(f) {
        if is_nested(f) {
            quote!(<#elem as #bnb::__private::FixedBitLen>::BIT_LEN)
        } else {
            quote!(<#elem as #bnb::__private::Bits>::BITS)
        }
    } else if is_nested(f) {
        quote!(<#ty as #bnb::__private::FixedBitLen>::BIT_LEN)
    } else if let Some(len) = byte_array_len(f) {
        quote!(((#len) as u32 * 8))
    } else {
        quote!(<#ty as #bnb::__private::Bits>::BITS)
    }
}

/// Positioning statements emitted before/after a field: `align_*` skips to the next
/// byte boundary, `pad_*` skips a bit count.
fn pad_read_tokens(align: bool, pad: Option<&syn::Expr>) -> TokenStream2 {
    let bnb = crate::bnb_path();
    let align = align.then(|| quote!(#bnb::__private::align_read(__bnb_r)?;));
    let pad = pad.map(|n| quote!(#bnb::__private::skip_read(__bnb_r, #n)?;));
    quote!(#align #pad)
}

fn pad_write_tokens(align: bool, pad: Option<&syn::Expr>) -> TokenStream2 {
    let bnb = crate::bnb_path();
    let align = align.then(|| quote!(#bnb::__private::align_write(__bnb_w)?;));
    let pad = pad.map(|n| quote!(#bnb::__private::skip_write(__bnb_w, #n)?;));
    quote!(#align #pad)
}

/// The decode statement for one field — a `let #id = …;` binding, wrapped with any
/// `pad_*`/`align_*` positioning. A later `count` can name an earlier field.
fn field_read_stmt(f: &syn::Field, br: &FieldBr) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let pre = pad_read_tokens(br.align_before, br.pad_before.as_ref());
    let post = pad_read_tokens(br.align_after, br.pad_after.as_ref());
    // `seek = <bits>`: jump to an absolute bit offset before the read (following a
    // pointer). Read-side only; emitted inside the `restore_position` wrap so the saved
    // offset is the *pre-seek* one (read at the offset, then return).
    let seek = br
        .seek
        .as_ref()
        .map(|e| quote!(#bnb::__private::Source::seek_to_bit(__bnb_r, (#e) as usize)?;));
    let mut core = field_read_core(f, br)?;
    // `dbg`: trace the field's start offset and decoded value (the field must be
    // `Debug`). Captured after any `seek`, so the offset is where the bits actually came
    // from. TRACE level under target `bnb::dbg` — enable with `RUST_LOG=bnb::dbg=trace`.
    if br.dbg {
        let id = f.ident.as_ref().expect("named field");
        core = quote! {
            let __dbg_at = #bnb::__private::Source::bit_pos(__bnb_r);
            #core
            #bnb::__private::tracing::trace!(
                target: "bnb::dbg",
                field = ::core::stringify!(#id),
                at_bit = __dbg_at,
                value = ?#id,
            );
        };
    }
    let mut body = quote!(#seek #core);
    if br.restore_position {
        // Peek: save the offset (before any seek), read the field, rewind so later
        // fields re-read from where they were.
        body = quote! {
            let __pos = #bnb::__private::Source::bit_pos(__bnb_r);
            #body
            #bnb::__private::Source::seek_to_bit(__bnb_r, __pos)?;
        };
    }
    Ok(quote!(#pre #body #post))
}

/// The core decode statement (without positioning).
fn field_read_core(f: &syn::Field, br: &FieldBr) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let id = f.ident.as_ref().expect("named field");
    let ty = &f.ty;
    // A `#[reserved]` field reads as a normal stored leaf here (so the actual wire bits
    // are observable and retained — decode is always verbatim).
    // `ignore`: in-memory only — `Default::default()` on read, no input consumed.
    if br.ignore {
        return Ok(quote!(let #id = ::core::default::Default::default();));
    }
    // `if(<cond>)`: a conditional `Option<T>`. `cond` is over earlier fields (as
    // locals). `Some(read)` when it holds, else `None` (consuming nothing).
    if let Some(cond) = &br.cond {
        let inner = option_elem(f).ok_or_else(|| {
            syn::Error::new_spanned(f, "`#[br(if(...))]` requires an `Option<_>` field")
        })?;
        let read_inner = if is_nested(f) {
            quote!(<#inner as #bnb::__private::BitDecode>::bit_decode(__bnb_r)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?)
        } else {
            quote! {{
                let __v: #inner = #bnb::__private::Source::read(__bnb_r)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
                __v
            }}
        };
        return Ok(quote! {
            let #id = if (#cond) {
                ::core::option::Option::Some(#read_inner)
            } else {
                ::core::option::Option::None
            };
        });
    }
    // `map`/`try_map`: read the wire value (`f`'s argument type) and transform it to
    // the field type, pinned to the field's declared type.
    if let Some(map) = &br.map {
        return Ok(
            quote!(let #id: #ty = #bnb::__private::read_mapped(__bnb_r, #map)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;),
        );
    }
    if let Some(try_map) = &br.try_map {
        return Ok(
            quote!(let #id: #ty = #bnb::__private::read_try_mapped(__bnb_r, #try_map)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;),
        );
    }
    // `parse_with`: the escape hatch — a custom `f(r) -> Result<T, BitError>`.
    if let Some(f) = &br.parse_with {
        return Ok(quote!(let #id: #ty = (#f)(__bnb_r)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;));
    }
    if let Some(elem) = vec_elem(f) {
        let count = br.count.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(f, "a `Vec<_>` field needs `#[br(count = <expr>)]`")
        })?;
        // Read one element into `__e`, pinning its type so inference can't drift.
        let read_elem = if let Some(names) = &br.ctx {
            let lit = ctx_literal(&ctx_struct_ty(elem)?, names, None);
            quote! {
                let __e = <#elem>::decode_with(__bnb_r, #lit)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            }
        } else if is_nested(f) {
            quote! {
                let __e = <#elem as #bnb::__private::BitDecode>::bit_decode(__bnb_r)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            }
        } else {
            quote! {
                let __e: #elem = #bnb::__private::Source::read(__bnb_r)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            }
        };
        // No untrusted pre-allocation: `count` is attacker-controlled, so grow the
        // Vec by pushing (bounded by the input — each element consumes ≥1 bit).
        Ok(quote! {
            let #id = {
                let __n = (#count) as usize;
                let mut __v: #bnb::__private::Vec<#elem> = #bnb::__private::Vec::new();
                for _ in 0..__n {
                    #read_elem
                    __v.push(__e);
                }
                __v
            };
        })
    } else {
        if br.count.is_some() {
            return Err(syn::Error::new_spanned(
                f,
                "`#[br(count = …)]` applies only to a `Vec<_>` field",
            ));
        }
        if let Some(names) = &br.ctx {
            let lit = ctx_literal(&ctx_struct_ty(ty)?, names, None);
            Ok(quote!(let #id = <#ty>::decode_with(__bnb_r, #lit)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;))
        } else if is_nested(f) {
            Ok(
                quote!(let #id = <#ty as #bnb::__private::BitDecode>::bit_decode(__bnb_r)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;),
            )
        } else if byte_array_len(f).is_some() {
            Ok(quote!(let #id = #bnb::__private::read_byte_array(__bnb_r)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;))
        } else {
            // Pin the leaf type explicitly: a `temp` field is not stored in `Self`,
            // so the construction can't infer it.
            Ok(
                quote!(let #id: #ty = __bnb_r.read().map_err(|e| e.in_field(::core::stringify!(#id)))?;),
            )
        }
    }
}

/// The encode statement for one field, wrapped with any `pad_*`/`align_*`. `Vec<T>`
/// writes every element; the count is implied by `len()` (a separate length field
/// is the user's, often `calc`'d). `field_set` is the parent's field names, for
/// resolving a `ctx { … }` pass.
fn field_write_stmt(
    f: &syn::Field,
    br: &FieldBr,
    field_set: &[&Ident],
    spec: bool,
) -> syn::Result<TokenStream2> {
    let pre = pad_write_tokens(br.align_before, br.pad_before.as_ref());
    let post = pad_write_tokens(br.align_after, br.pad_after.as_ref());
    // A `restore_position` field is a read-side peek (it overlaps later data), so it
    // is not written — the overlapping field emits those bytes.
    let core = if br.restore_position {
        quote!()
    } else {
        field_write_core(f, br, field_set, spec)?
    };
    Ok(quote!(#pre #core #post))
}

/// The core encode statement (without positioning).
fn field_write_core(
    f: &syn::Field,
    br: &FieldBr,
    field_set: &[&Ident],
    spec: bool,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let id = f.ident.as_ref().expect("named field");
    let ty = &f.ty;
    // `#[reserved]` on the **canonical** path: write the spec value (type zero, or the
    // `reserved_with` expression). On the verbatim (default) path a reserved field falls
    // through and writes its stored value like any field.
    if spec {
        if let Some(value) = reserved_spec_value(f)? {
            return Ok(quote!(#bnb::__private::Sink::write(__bnb_w, #value)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;));
        }
    }
    // `ignore`: in-memory only — emit nothing.
    if br.ignore {
        return Ok(quote!());
    }
    // `calc`: a value computed from the other fields. On the **canonical** path
    // (`spec == true`) we recompute it; on the default **verbatim** path a *stored*
    // (non-`temp`) `calc` field is written as-is — `to_bytes` never silently rewrites what
    // the caller put in the field (dual-use). A `temp` field has no stored value, so it
    // always recomputes.
    if let Some(calc) = &br.calc {
        if br.temp {
            // Not stored, so a later `#[br(ctx { … })]` pass can't resolve it via
            // `self.#id`. Bind the computed value to a **named** local (in encode-fn scope,
            // in declaration order) so the ctx pass finds it — e.g. a tag recomputed with
            // `#[bw(calc = self.body.tag())]` and handed to a `tag`-dispatched enum.
            return Ok(quote! {
                let #id: #ty = #calc;
                #bnb::__private::Sink::write(__bnb_w, #id)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            });
        }
        if spec {
            // Canonical: recompute from the other fields, pinned to the declared type.
            return Ok(quote! {
                {
                    let __calc: #ty = #calc;
                    #bnb::__private::Sink::write(__bnb_w, __calc)
                        .map_err(|e| e.in_field(::core::stringify!(#id)))?;
                }
            });
        }
        // Verbatim (default): write the stored value exactly as it is.
        return Ok(quote!(#bnb::__private::Sink::write(__bnb_w, self.#id)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;));
    }
    // A `temp` field is never stored, so it cannot be written without a `calc`.
    if br.temp {
        return Err(syn::Error::new_spanned(
            f,
            "a `#[br(temp)]` field is not stored, so it needs `#[bw(calc = <expr>)]` to encode",
        ));
    }
    // `map`: write `f(&self.field)` (the wire value). A read-side `map`/`try_map`
    // needs the inverse `#[bw(map = …)]` to be encodable.
    if let Some(bw_map) = &br.bw_map {
        return Ok(
            quote!(#bnb::__private::write_mapped(__bnb_w, &self.#id, #bw_map)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;),
        );
    }
    // `write_with`: the escape hatch — a custom `f(&self.field, w) -> Result<()>`.
    if let Some(f) = &br.write_with {
        return Ok(quote!((#f)(&self.#id, __bnb_w)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;));
    }
    if br.map.is_some() || br.try_map.is_some() {
        return Err(syn::Error::new_spanned(
            f,
            "a `#[br(map = …)]`/`#[br(try_map = …)]` field needs the inverse `#[bw(map = <f>)]` to encode",
        ));
    }
    if br.parse_with.is_some() {
        return Err(syn::Error::new_spanned(
            f,
            "a `#[br(parse_with = …)]` field needs the inverse `#[bw(write_with = <f>)]` to encode",
        ));
    }
    // `if(...)`: a conditional `Option<T>` — write the inner value iff present (the
    // `Option` drives the write; the read-side condition is not re-evaluated).
    if br.cond.is_some() {
        let inner = option_elem(f).ok_or_else(|| {
            syn::Error::new_spanned(f, "`#[br(if(...))]` requires an `Option<_>` field")
        })?;
        let write_inner = if is_nested(f) {
            quote!(<#inner as #bnb::__private::BitEncode>::bit_encode(__v, __bnb_w)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else {
            quote!(#bnb::__private::Sink::write(__bnb_w, *__v)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        };
        return Ok(quote! {
            if let ::core::option::Option::Some(__v) = &self.#id {
                #write_inner
            }
        });
    }
    if let Some(elem) = vec_elem(f) {
        let write_elem = if let Some(names) = &br.ctx {
            let elem_ctx = ctx_struct_ty(elem)?;
            let lit = ctx_literal(&elem_ctx, names, Some(field_set));
            quote!(<#elem as #bnb::EncodeWith<#elem_ctx>>::encode_with(__e, __bnb_w, #lit)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else if is_nested(f) {
            quote!(<#elem as #bnb::__private::BitEncode>::bit_encode(__e, __bnb_w)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else {
            quote!(#bnb::__private::Sink::write(__bnb_w, *__e)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        };
        Ok(quote! {
            for __e in &self.#id {
                #write_elem
            }
        })
    } else if let Some(names) = &br.ctx {
        let child_ctx = ctx_struct_ty(ty)?;
        let lit = ctx_literal(&child_ctx, names, Some(field_set));
        Ok(
            quote!(<#ty as #bnb::EncodeWith<#child_ctx>>::encode_with(&self.#id, __bnb_w, #lit)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;),
        )
    } else if is_nested(f) {
        Ok(
            quote!(<#ty as #bnb::__private::BitEncode>::bit_encode(&self.#id, __bnb_w)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;),
        )
    } else if byte_array_len(f).is_some() {
        Ok(quote!(#bnb::__private::write_byte_array(&self.#id, __bnb_w)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;))
    } else {
        Ok(quote!(__bnb_w.write(self.#id).map_err(|e| e.in_field(::core::stringify!(#id)))?;))
    }
}

/// A const-eval assertion that the struct is *not* entirely byte-aligned (the
/// bit-stream codec would otherwise be the wrong tool). Empty/opted-out → no guard.
/// A sub-byte `magic` counts as a non-byte-aligned element, so it suppresses the
/// guard just like a sub-byte field.
fn alignment_guard(fields: &FieldsNamed, allow: bool, magic: Option<&syn::Expr>) -> TokenStream2 {
    if allow || (fields.named.is_empty() && magic.is_none()) {
        return quote!();
    }
    let bnb = crate::bnb_path();
    let mut terms: Vec<TokenStream2> = fields
        .named
        .iter()
        .map(|f| {
            let w = field_width(f);
            quote!((#w % 8 == 0))
        })
        .collect();
    if let Some(m) = magic {
        terms.push(quote!((#bnb::__private::bits_of(&#m) % 8 == 0)));
    }
    quote! {
        const _: () = {
            assert!(!(true #(&& #terms)*), #BYTE_ALIGNED_MSG);
        };
    }
}

pub(crate) fn expand_decode(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match decode_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn decode_inner(input: &DeriveInput) -> syn::Result<TokenStream2> {
    gen_decode(
        &input.ident,
        named_struct(input)?,
        &parse_bit_stream(input)?,
        false, // the bare derive never injects the `encode_mode` field
    )
}

/// Generates the decode side (`BitDecode` + entry points, or `decode_with` for a
/// `ctx` type) from a name + field list + parsed options. Shared by the
/// `#[derive(BitDecode)]` path and by `#[bin]` (which can pass `temp` fields not
/// present in the emitted struct).
fn gen_decode(
    name: &Ident,
    fields: &FieldsNamed,
    attrs: &BitStreamAttrs,
    inject_mode: bool,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    // When `#[bin]` injects the in-memory `encode_mode` field (a message with a
    // verbatim/canonical distinction), every `Self { … }` the decoder builds must set it.
    // A decoded value defaults to `Verbatim`, so `decode` then `encode` round-trips.
    let mode_init = if inject_mode {
        quote!(, encode_mode: #bnb::EncodeMode::Verbatim)
    } else {
        quote!()
    };
    // Parse each field's `#[br]`/`#[bw]` directives once, up front (propagating any
    // parse error immediately), then drive every decision off the parsed list — no
    // re-parsing per predicate.
    let brs: Vec<FieldBr> = fields
        .named
        .iter()
        .map(parse_field_br)
        .collect::<syn::Result<Vec<_>>>()?;
    // A `ctx`/`if`/map/positioning field anywhere makes widths/alignment
    // indeterminable: exempt from the guard and never `FixedBitLen`.
    let indeterminate = !attrs.ctx.is_empty() || brs.iter().any(br_indeterminate);
    let guard = alignment_guard(
        fields,
        attrs.allow_byte_aligned || indeterminate,
        attrs.magic.as_ref(),
    );
    let layout = layout_token(attrs);
    // `magic`: a leading constant read and verified before the fields. Its width
    // (inferred from the value's type) joins `BIT_LEN`.
    let (magic_read, magic_bits) = match &attrs.magic {
        Some(m) => (
            quote! {
                #bnb::__private::verify_magic(__bnb_r, #m).map_err(|e| e.in_field("magic"))?;
            },
            quote!(#bnb::__private::bits_of(&#m) +),
        ),
        None => (quote!(), quote!()),
    };

    // Read each field into a same-named local (declaration order), so a later
    // `count`/`ctx` directive can reference an earlier field; then build `Self`
    // from the **stored** fields only (`#[br(temp)]` reads into a local but is not
    // a struct field).
    let ids: Vec<&Ident> = fields
        .named
        .iter()
        .zip(&brs)
        .filter(|(_, br)| !br.temp)
        .map(|(f, _)| f.ident.as_ref().expect("named field"))
        .collect();
    let read_stmts = fields
        .named
        .iter()
        .zip(&brs)
        .map(|(f, br)| field_read_stmt(f, br))
        .collect::<syn::Result<Vec<_>>>()?;

    // A `ctx(...)`-declaring message takes context it can't get from the plain
    // `BitDecode` trait, so it gets inherent `decode_with`/`decode_with_exact`
    // (binding the ctx params as locals) instead — no `BitDecode`/`FixedBitLen`.
    if !attrs.ctx.is_empty() {
        let ctx_name = ctx_struct_ident(name);
        let ctx_binds = attrs.ctx.iter().map(|(n, _)| quote!(let #n = __ctx.#n;));
        return Ok(quote! {
            #guard
            impl #name {
                #[doc = "Decode from a bit source, given the context this type declares via `ctx(...)`."]
                #[allow(unused_variables)] // a ctx param may be used on only one side
                pub fn decode_with<S: #bnb::__private::Source>(
                    __bnb_r: &mut S,
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #(#ctx_binds)*
                    #magic_read
                    #(#read_stmts)*
                    ::core::result::Result::Ok(Self { #(#ids),* #mode_init })
                }
                #[doc = "Decode from bytes with context, requiring every whole byte consumed."]
                pub fn decode_with_exact(
                    bytes: &[u8],
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_exact_with(bytes, #layout, |__bnb_r| Self::decode_with(__bnb_r, __ctx))
                }
            }
            // ctx Layer 2: the polymorphic companion, so generic combinators can take
            // this type via `T: DecodeWith<#ctx_name>`.
            impl #bnb::DecodeWith<#ctx_name> for #name {
                fn decode_with<S: #bnb::__private::Source>(
                    __bnb_r: &mut S,
                    args: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    <#name>::decode_with(__bnb_r, args)
                }
            }
        });
    }

    // A message with a `count`-driven `Vec` (or a `ctx`/`if` field) is variable-
    // length; only a fixed one also implements `FixedBitLen` (sizes embedded regions).
    let variable = indeterminate || fields.named.iter().any(|f| vec_elem(f).is_some());
    let fixed_bit_len = if variable {
        quote!()
    } else {
        let widths = fields.named.iter().map(field_width);
        quote! {
            impl #bnb::__private::FixedBitLen for #name {
                const BIT_LEN: u32 = #magic_bits 0 #(+ #widths)*;
            }
        }
    };

    // `restore_position` seeks, so the explicit-source entry point requires a
    // [`SeekSource`]; a forward-only stream is then a compile error. Without a seek,
    // any forward `Source` (including a streaming reader) works. The slice entry
    // points (`decode`/`peek`/`decode_exact`) always go through a seekable
    // `BitReader`, so they are unaffected.
    let seeks = brs
        .iter()
        .any(|br| br.restore_position || br.seek.is_some());
    let from_bound = if seeks {
        quote!(#bnb::__private::SeekSource)
    } else {
        quote!(#bnb::__private::Source)
    };
    let from_doc = if seeks {
        "Decode from an explicit **seekable** bit source. This message uses a seeking \
         directive (`restore_position`/`seek`), so a forward-only stream is rejected at \
         compile time."
    } else {
        "Decode from an explicit bit source (a `BitReader` cursor or a streaming reader)."
    };

    // There is no canonical *decode*: `decode_*` is always verbatim (it retains the wire
    // bits of reserved fields — dual-use). Canonicalization is an encode-side concern
    // (`to_canonical_bytes`) or an explicit in-memory helper.

    Ok(quote! {
        #guard
        #fixed_bit_len
        impl #bnb::BitDecode for #name {
            fn bit_decode<S: #bnb::__private::Source>(
                __bnb_r: &mut S,
            ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                #magic_read
                #(#read_stmts)*
                ::core::result::Result::Ok(Self { #(#ids),* #mode_init })
            }
        }

        impl #name {
            #[doc = "Decode one message from the front of `buf`, advancing it past the bytes consumed (the tail stays in `buf`; transactional on error)."]
            pub fn decode(buf: &mut &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                #bnb::__private::decode_consume(buf, #layout)
            }
            #[doc = "Decode one message from `bytes` without consuming the caller's buffer (tail-tolerant)."]
            pub fn peek(bytes: &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                #bnb::__private::decode_peek(bytes, #layout)
            }
            #[doc = "Decode and require every whole byte consumed (errors with `ErrorKind::TrailingBytes` otherwise)."]
            pub fn decode_exact(bytes: &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                #bnb::__private::decode_exact(bytes, #layout)
            }
            #[doc = #from_doc]
            pub fn decode_from<S: #from_bound>(
                __bnb_r: &mut S,
            ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                <Self as #bnb::BitDecode>::bit_decode(__bnb_r)
            }
        }
    })
}

/// Whether a token stream mentions any of `names` (recursing into groups). Decides whether
/// a type's generated **encode** body reads a `ctx` parameter — and so whether `ctx` is
/// decode-only for it (plain encode) or it needs `encode_with`. Scanning the emitted tokens
/// catches *every* write-side reference (`calc`/`bw(map)`/`write_with`/`reserved_with`/
/// positioning, or a `ctx { … }` forward passing a param down) with no false negatives;
/// over-detection is harmless (it would merely keep `encode_with`).
fn tokens_mention(ts: TokenStream2, names: &[&Ident]) -> bool {
    ts.into_iter().any(|tt| match tt {
        proc_macro2::TokenTree::Ident(id) => names.iter().any(|n| **n == id),
        proc_macro2::TokenTree::Group(g) => tokens_mention(g.stream(), names),
        _ => false,
    })
}

pub(crate) fn expand_encode(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match encode_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn encode_inner(input: &DeriveInput) -> syn::Result<TokenStream2> {
    gen_encode(
        &input.ident,
        named_struct(input)?,
        &parse_bit_stream(input)?,
        false, // the bare derive never injects the `encode_mode` field
    )
}

/// Generates the encode side (`BitEncode` + entry points, or `encode_with` for a
/// `ctx` type). Shared by `#[derive(BitEncode)]` and `#[bin]`. `calc` fields write
/// a computed value; `temp` fields (no `self` field) are written via their `calc`.
fn gen_encode(
    name: &Ident,
    fields: &FieldsNamed,
    attrs: &BitStreamAttrs,
    inject_mode: bool,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    // Parse each field's directives once (see `gen_decode`), then derive everything
    // from the parsed list.
    let brs: Vec<FieldBr> = fields
        .named
        .iter()
        .map(parse_field_br)
        .collect::<syn::Result<Vec<_>>>()?;
    let indeterminate = !attrs.ctx.is_empty() || brs.iter().any(br_indeterminate);
    let guard = alignment_guard(
        fields,
        attrs.allow_byte_aligned || indeterminate,
        attrs.magic.as_ref(),
    );
    let layout = layout_token(attrs);
    // `magic`: emit the leading constant before the fields (matched read/write).
    let magic_write = match &attrs.magic {
        Some(m) => quote! {
            #bnb::__private::Sink::write(__bnb_w, #m).map_err(|e| e.in_field("magic"))?;
        },
        None => quote!(),
    };
    // Only stored (non-`temp`) fields exist on `self`, for `ctx { … }` resolution.
    let field_set: Vec<&Ident> = fields
        .named
        .iter()
        .zip(&brs)
        .filter(|(_, br)| !br.temp)
        .map(|(f, _)| f.ident.as_ref().expect("named field"))
        .collect();
    let writes = fields
        .named
        .iter()
        .zip(&brs)
        .map(|(f, br)| field_write_stmt(f, br, &field_set, false))
        .collect::<syn::Result<Vec<_>>>()?;

    // `ctx` is **decode-only** by default: if the generated encode body references a ctx
    // param — a `calc`/`bw(map)`/`write_with`/`reserved_with`/positioning expr, or a
    // `ctx { … }` forward passing one down — the type gets `encode_with`/`to_bytes_with`;
    // otherwise a plain `BitEncode`/`to_bytes` (below), so encode needs no context.
    let ctx_names: Vec<&Ident> = attrs.ctx.iter().map(|(n, _)| n).collect();
    let encode_uses_ctx =
        !ctx_names.is_empty() && writes.iter().any(|w| tokens_mention(w.clone(), &ctx_names));
    if encode_uses_ctx {
        let ctx_name = ctx_struct_ident(name);
        let ctx_binds = attrs.ctx.iter().map(|(n, _)| quote!(let #n = __ctx.#n;));
        return Ok(quote! {
            #guard
            impl #name {
                #[doc = "Encode to a bit sink, given the context this type declares via `ctx(...)`."]
                #[allow(unused_variables)] // a ctx param may be used on only one side
                pub fn encode_with<K: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut K,
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    #(#ctx_binds)*
                    #magic_write
                    #(#writes)*
                    ::core::result::Result::Ok(())
                }
                #[doc = "Encode to a `Vec<u8>` with context."]
                pub fn to_bytes_with(
                    &self,
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                    #bnb::__private::encode_to_vec_with(#layout, |__bnb_w| self.encode_with(__bnb_w, __ctx))
                }
            }
            // ctx Layer 2: the polymorphic companion (dual of `DecodeWith`).
            impl #bnb::EncodeWith<#ctx_name> for #name {
                fn encode_with<K: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut K,
                    args: #ctx_name,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    <#name>::encode_with(self, __bnb_w, args)
                }
            }
        });
    }

    // The **canonical** encoder: reserved fields written as their spec value and `calc`
    // fields recomputed (ignoring the stored values). Generated only when it would differ
    // from the verbatim encoder — i.e. the message has a reserved field or a non-`temp`
    // `calc` field (otherwise canonical and verbatim are identical).
    let has_canonical = fields
        .named
        .iter()
        .zip(&brs)
        .any(|(f, br)| field_is_reserved(f) || (br.calc.is_some() && !br.temp));
    // When `#[bin]` injected the in-memory `encode_mode` field, `BitEncode::encode_mode`
    // returns it (so `encode` consults the value's mode) and `to_canonical` carries it over.
    let encode_mode_override = if inject_mode {
        quote! {
            fn encode_mode(&self) -> #bnb::EncodeMode { self.encode_mode }
        }
    } else {
        quote!()
    };
    let canonical_mode_init = if inject_mode {
        quote!(, encode_mode: self.encode_mode)
    } else {
        quote!()
    };
    let (canonical_method, canonical_inherent) = if !has_canonical {
        (quote!(), quote!())
    } else {
        let writes_canonical = fields
            .named
            .iter()
            .zip(&brs)
            .map(|(f, br)| field_write_stmt(f, br, &field_set, true))
            .collect::<syn::Result<Vec<_>>>()?;

        // In-memory canonicalization helpers (`to_canonical`/`canonical_diff`/
        // `is_canonical`). For each *stored* (non-`temp`) field the canonical value is: the
        // recomputed `calc` expr, the reserved spec value, or the field itself unchanged.
        let mut calc_precompute = Vec::new();
        let mut field_inits = Vec::new();
        let mut diff_checks = Vec::new();
        for (f, br) in fields.named.iter().zip(&brs) {
            if br.temp {
                continue; // not stored — absent from the struct
            }
            let id = f.ident.as_ref().expect("named field");
            let ty = &f.ty;
            if let Some(calc) = &br.calc {
                // Non-`temp` `calc` (temp filtered above): canonical value = recompute.
                let local = format_ident!("__canon_{}", id);
                calc_precompute.push(quote!(let #local: #ty = #calc;));
                field_inits.push(quote!(#id: #local));
                diff_checks
                    .push(quote!(if self.#id != (#calc) { __d.push(::core::stringify!(#id)); }));
            } else if let Some(spec) = reserved_spec_value(f)? {
                // Reserved: canonical value = spec value.
                field_inits.push(quote!(#id: #spec));
                diff_checks
                    .push(quote!(if self.#id != (#spec) { __d.push(::core::stringify!(#id)); }));
            } else {
                // Ordinary stored field: moved unchanged.
                field_inits.push(quote!(#id: self.#id));
            }
        }

        // Overrides `BitEncode`'s default (verbatim) `canonical_bit_encode`.
        let method = quote! {
            fn canonical_bit_encode<K: #bnb::__private::Sink>(
                &self,
                __bnb_w: &mut K,
            ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                #magic_write
                #(#writes_canonical)*
                ::core::result::Result::Ok(())
            }
        };
        let inherent = quote! {
            impl #name {
                #[doc = "Encode the **canonical** form to a `Vec<u8>`: reserved fields written as their"]
                #[doc = "spec value and `calc` fields recomputed (ignoring the stored values), so the"]
                #[doc = "result is always spec-compliant. (`to_bytes` is verbatim — it writes exactly"]
                #[doc = "what is stored.) To make the `std::io::Write` `encode(&mut w)` emit this form,"]
                #[doc = "set the value's `encode_mode` to `Canonical` (e.g. `with_encode_mode`)."]
                pub fn to_canonical_bytes(&self) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                    #bnb::__private::encode_to_vec_with(
                        #layout,
                        |__bnb_w| <Self as #bnb::BitEncode>::canonical_bit_encode(self, __bnb_w),
                    )
                }
                #[doc = "Encode the canonical form into an explicit bit sink."]
                pub fn canonical_encode_into<K: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut K,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    <Self as #bnb::BitEncode>::canonical_bit_encode(self, __bnb_w)
                }

                #[doc = "The **canonical form in memory**: a copy with reserved fields set to their"]
                #[doc = "spec value and `calc` fields recomputed. `value.to_canonical().to_bytes()`"]
                #[doc = "equals `value.to_canonical_bytes()`."]
                pub fn to_canonical(self) -> Self {
                    #(#calc_precompute)*
                    Self { #(#field_inits),* #canonical_mode_init }
                }

                #[doc = "The names of the stored fields whose value differs from canonical — i.e."]
                #[doc = "reserved fields not at their spec value, or `calc` fields not equal to their"]
                #[doc = "recomputed value. Empty iff `self` is already canonical."]
                pub fn canonical_diff(&self) -> #bnb::__private::Vec<&'static str> {
                    let mut __d = #bnb::__private::Vec::new();
                    #(#diff_checks)*
                    __d
                }

                #[doc = "Whether `self` is already in canonical form (no reserved/`calc` field differs)."]
                pub fn is_canonical(&self) -> bool {
                    self.canonical_diff().is_empty()
                }
            }
        };
        (method, inherent)
    };

    // A `ctx` type whose encode does *not* read context still impls `EncodeWith` (ignoring
    // the context), so a parent can forward to it uniformly whether or not it needs one.
    let encode_with_trait = if attrs.ctx.is_empty() {
        quote!()
    } else {
        let ctx_name = ctx_struct_ident(name);
        quote! {
            impl #bnb::EncodeWith<#ctx_name> for #name {
                #[allow(unused_variables)]
                fn encode_with<K: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut K,
                    args: #ctx_name,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    <Self as #bnb::BitEncode>::bit_encode(self, __bnb_w)
                }
            }
        }
    };

    Ok(quote! {
        #guard
        impl #bnb::BitEncode for #name {
            const LAYOUT: #bnb::Layout = #layout;
            fn bit_encode<K: #bnb::__private::Sink>(
                &self,
                __bnb_w: &mut K,
            ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                #magic_write
                #(#writes)*
                ::core::result::Result::Ok(())
            }
            #canonical_method
            #encode_mode_override
        }

        impl #name {
            #[doc = "Encode to a `Vec<u8>`, **verbatim** — exactly what's stored, never silently"]
            #[doc = "rewritten (so `decode` then `to_bytes` round-trips byte-for-byte). For the"]
            #[doc = "spec-normalized form, use `to_canonical_bytes` (generated when the message has"]
            #[doc = "a `reserved` or `calc` field). To write to a `std::io::Write` sink following the"]
            #[doc = "value's `encode_mode`, bring [`EncodeExt`](::bnb::EncodeExt) into scope and call"]
            #[doc = "`.encode(&mut w)` (the `std` feature)."]
            pub fn to_bytes(&self) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                #bnb::__private::encode_to_vec(self, #layout)
            }
            #[doc = "Encode (verbatim) into an explicit bit sink (a `BitWriter`)."]
            pub fn encode_into<K: #bnb::__private::Sink>(
                &self,
                __bnb_w: &mut K,
            ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                <Self as #bnb::BitEncode>::bit_encode(self, __bnb_w)
            }
        }
        #encode_with_trait
        #canonical_inherent
    })
}

// ---------------------------------------------------------------------------
// `#[bin]` — the unified codec attribute.
//
// One macro that folds codec + builder. It *lowers* to the existing
// `#[derive(BitDecode, BitEncode, BitsBuilder)]` + `#[bit_stream(...)]`, so the
// field-directive logic lives in those derives and `#[bin]` is a thin, zero-
// duplication front-end over them.
// Field directives (`#[br]`/`#[bw]`/`#[brw]`) ride through as derive helper attrs.
// ---------------------------------------------------------------------------

/// Parsed struct-level `#[bin(...)]` options.
#[derive(Default)]
struct BinArgs {
    read_only: bool,
    write_only: bool,
    no_builder: bool,
    forward_only: bool,
    lsb: bool,
    little: bool,
    magic: Option<syn::Expr>,
    ctx: Vec<(Ident, Type)>,
    /// `validate = <path>` — a `fn(&Self) -> Result<(), impl Display>` run by
    /// `build()` (construction soundness; the parser stays permissive). A free
    /// function, not a method, so it isn't mistaken for protocol-context validity.
    validate: Option<syn::Path>,
    /// `tag = <ctx-param>` (enum only) — the **selector**: dispatch each `#[bin(tag =
    /// <value>)]` variant on this `ctx(...)` parameter (read-only, never on the wire).
    tag: Option<Ident>,
}

/// Entry for `#[bin(...)]`.
pub(crate) fn expand_bin(attr: TokenStream, item: TokenStream) -> TokenStream {
    match bin_inner(attr, item) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn bin_inner(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream2> {
    let mut args = BinArgs::default();
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("read_only") {
            args.read_only = true;
        } else if meta.path.is_ident("write_only") {
            args.write_only = true;
        } else if meta.path.is_ident("no_builder") {
            args.no_builder = true;
        } else if meta.path.is_ident("forward_only") {
            args.forward_only = true;
        } else if meta.path.is_ident("bit_order") {
            let v: Ident = meta.value()?.parse()?;
            match v.to_string().as_str() {
                "msb" => args.lsb = false,
                "lsb" => args.lsb = true,
                _ => return Err(meta.error("expected `msb` or `lsb`")),
            }
        } else if meta.path.is_ident("big") {
            args.little = false;
        } else if meta.path.is_ident("little") {
            args.little = true;
        } else if meta.path.is_ident("magic") {
            args.magic = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("ctx") {
            let content;
            syn::parenthesized!(content in meta.input);
            let params = Punctuated::<CtxParam, Token![,]>::parse_terminated(&content)?;
            args.ctx = params.into_iter().map(|p| (p.name, p.ty)).collect();
        } else if meta.path.is_ident("validate") {
            args.validate = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("tag") {
            args.tag = Some(meta.value()?.parse()?);
        } else {
            return Err(meta.error(
                "unknown `#[bin(...)]` option; expected one of: read_only, write_only, \
                 no_builder, forward_only, big, little, bit_order = msb|lsb, magic = <expr>, \
                 ctx(name: Ty, …), validate = <path>, tag = <ctx-param>",
            ));
        }
        Ok(())
    });
    Parser::parse(parser, attr)?;

    if args.read_only && args.write_only {
        return Err(syn::Error::new(
            ::proc_macro2::Span::call_site(),
            "`read_only` and `write_only` are mutually exclusive",
        ));
    }
    match syn::parse::<syn::Item>(item)? {
        syn::Item::Struct(s) => bin_struct(&args, &s),
        syn::Item::Enum(e) => bin_enum(&args, &e),
        other => Err(syn::Error::new_spanned(
            other,
            "#[bin] requires a struct or an enum",
        )),
    }
}

/// The `#[bin]` struct path: the codec (`BitDecode`/`BitEncode`) and the
/// required-by-default builder, folded over a named-field struct.
fn bin_struct(args: &BinArgs, s: &ItemStruct) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    if args.tag.is_some() {
        return Err(syn::Error::new_spanned(
            &s.ident,
            "`tag` (variant dispatch) applies to a `#[bin]` enum, not a struct",
        ));
    }
    if !s.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &s.generics,
            "#[bin] does not support generic parameters yet",
        ));
    }
    let full_fields = match &s.fields {
        Fields::Named(n) => n,
        _ => {
            return Err(syn::Error::new_spanned(
                &s.ident,
                "#[bin] requires a struct with named fields",
            ));
        }
    };
    // `forward_only` pins a `Source`-only bound: a seek directive is then a compile
    // error (it would need a `SeekSource`).
    if args.forward_only {
        for f in &full_fields.named {
            let Ok(br) = parse_field_br(f) else { continue };
            let seeking = if br.restore_position {
                Some("restore_position")
            } else if br.seek.is_some() {
                Some("seek = …")
            } else {
                None
            };
            if let Some(name) = seeking {
                return Err(syn::Error::new_spanned(
                    f,
                    format!(
                        "`#[br({name})]` needs to seek, but the struct is `#[bin(forward_only)]`"
                    ),
                ));
            }
        }
    }

    // `#[bin]` generates the codec **directly** from the full field list — so a
    // `#[br(temp)]` field (read into a local, not stored) can participate — while
    // the emitted struct drops it. (Unlike P2.0–P2.3, this no longer lowers to the
    // `#[derive(BitDecode/BitEncode)]` codec; those derives remain usable directly.)
    //
    // The right-tool guard is always suppressed for `#[bin]`: it is the *unified*
    // codec, so a byte-aligned message is a first-class use, not a misuse. The
    // guard stays on the bare derives as advisory steering toward `#[bin]`.
    let attrs = BitStreamAttrs {
        allow_byte_aligned: true,
        lsb: args.lsb,
        little: args.little,
        magic: args.magic.clone(),
        ctx: args.ctx.clone(),
    };
    // A message with a verbatim/canonical distinction carries a settable, wire-ignored
    // `encode_mode` field (consulted by `encode`). Only inject it on the write side: a
    // `read_only` codec never encodes, so it needs no mode.
    let inject_mode = !args.read_only && struct_has_canonical(full_fields);
    if inject_mode {
        if let Some(clash) = full_fields
            .named
            .iter()
            .find(|f| f.ident.as_ref().is_some_and(|i| i == "encode_mode"))
        {
            return Err(syn::Error::new_spanned(
                clash,
                "`#[bin]` adds an `encode_mode` field to a message with a reserved/calc field; \
                 rename this field",
            ));
        }
    }

    let decode = if args.write_only {
        quote!()
    } else {
        gen_decode(&s.ident, full_fields, &attrs, inject_mode)?
    };
    let encode = if args.read_only {
        quote!()
    } else {
        gen_encode(&s.ident, full_fields, &attrs, inject_mode)?
    };

    // The emitted struct: drop `#[br(temp)]` fields (not stored) and strip codec-only
    // field attributes (they are not registered helper attrs here — the codec is
    // generated directly, not via the derives). A `#[reserved]` field is kept (it is a
    // normal stored field now), with its `#[reserved]`/`#[reserved_with]` attr stripped.
    let mut clean = s.clone();
    if let Fields::Named(named) = &mut clean.fields {
        named.named = named
            .named
            .iter()
            .filter(|f| !field_is_temp(f))
            .cloned()
            .map(|mut f| {
                f.attrs.retain(|a| !is_codec_field_attr(a));
                f
            })
            .collect();
    }

    // Inject the wire-ignored `encode_mode` field + its accessors, and re-emit `Debug`/
    // `PartialEq`/`Hash` as custom impls that exclude it (so a decoded value still equals a
    // wire-identical built one, regardless of mode). Construction is then builder/`decode`
    // only — a bare `Name { … }` literal can't name the private field.
    let mode_extras = if inject_mode {
        let BinDerives {
            kept,
            others,
            has_debug,
            has_partial_eq,
            has_hash,
        } = split_bin_derives(&clean.attrs)?;
        clean.attrs = others;
        if !kept.is_empty() {
            clean
                .attrs
                .extend(syn::Attribute::parse_outer.parse2(quote!(#[derive(#(#kept),*)]))?);
        }
        if let Fields::Named(named) = &mut clean.fields {
            named
                .named
                .push(syn::Field::parse_named.parse2(quote!(encode_mode: #bnb::EncodeMode))?);
        }
        // The stored, user-visible fields (everything in `clean` except the injected mode).
        let user_idents: Vec<_> = match &clean.fields {
            Fields::Named(n) => n
                .named
                .iter()
                .filter_map(|f| f.ident.clone())
                .filter(|i| i != "encode_mode")
                .collect(),
            _ => Vec::new(),
        };
        let name = &s.ident;
        let debug_impl = has_debug.then(|| {
            quote! {
                impl ::core::fmt::Debug for #name {
                    fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        __f.debug_struct(::core::stringify!(#name))
                            #(.field(::core::stringify!(#user_idents), &self.#user_idents))*
                            .finish()
                    }
                }
            }
        });
        let partial_eq_impl = has_partial_eq.then(|| {
            quote! {
                impl ::core::cmp::PartialEq for #name {
                    fn eq(&self, __other: &Self) -> bool {
                        true #(&& self.#user_idents == __other.#user_idents)*
                    }
                }
            }
        });
        let hash_impl = has_hash.then(|| {
            quote! {
                impl ::core::hash::Hash for #name {
                    fn hash<__H: ::core::hash::Hasher>(&self, __state: &mut __H) {
                        #(::core::hash::Hash::hash(&self.#user_idents, __state);)*
                    }
                }
            }
        });
        let vis = &s.vis;
        quote! {
            impl #name {
                #[doc = "The form `encode` writes for this value (`to_bytes`/`to_canonical_bytes` ignore it)."]
                #vis fn encode_mode(&self) -> #bnb::EncodeMode { self.encode_mode }
                #[doc = "Set the form `encode` writes (in place)."]
                #vis fn set_encode_mode(&mut self, __mode: #bnb::EncodeMode) { self.encode_mode = __mode; }
                #[doc = "Return `self` with the encode mode set (chainable)."]
                #[must_use]
                #vis fn with_encode_mode(mut self, __mode: #bnb::EncodeMode) -> Self {
                    self.encode_mode = __mode;
                    self
                }
            }
            #debug_impl
            #partial_eq_impl
            #hash_impl
        }
    } else {
        quote!()
    };

    // A positional `new(...)` over the stored fields, in declaration order — the direct
    // replacement for a struct literal (which an `encode_mode`-bearing type can no longer use,
    // and which is otherwise just sugar). Unlike the builder, it takes every stored field
    // (`reserved`/`calc` included) and never validates; the encode mode starts at `Verbatim`.
    let new_ctor = {
        let stored: Vec<&syn::Field> = full_fields
            .named
            .iter()
            .filter(|f| !field_is_temp(f))
            .collect();
        let params = stored.iter().map(|f| {
            let id = f.ident.as_ref().expect("named field");
            let ty = &f.ty;
            quote!(#id: #ty)
        });
        let inits = stored
            .iter()
            .map(|f| f.ident.as_ref().expect("named field"));
        let mode_field_init = if inject_mode {
            quote!(, encode_mode: #bnb::EncodeMode::Verbatim)
        } else {
            quote!()
        };
        let vis = &s.vis;
        let name = &s.ident;
        quote! {
            impl #name {
                #[doc = "Construct from every stored field, in declaration order — the direct"]
                #[doc = "replacement for a struct literal. The builder (`Self::builder()`) is the"]
                #[doc = "alternative that lets `reserved`/`#[builder(default)]` fields default; this"]
                #[doc = "takes them all and never validates. (The encode mode starts at `Verbatim`.)"]
                #[allow(clippy::too_many_arguments)]
                #vis fn new(#(#params),*) -> Self {
                    Self { #(#inits),* #mode_field_init }
                }
            }
        }
    };

    // The builder is generated directly from the stored fields (so it can run the
    // `validate` hook via `builder::generate`'s post_build). `temp` fields are absent;
    // a reserved field is present but optional, defaulting to its spec value.
    let builder = if args.read_only || args.no_builder {
        if args.validate.is_some() {
            return Err(syn::Error::new_spanned(
                &s.ident,
                "`validate` needs the builder; it is incompatible with `read_only`/`no_builder`",
            ));
        }
        quote!()
    } else {
        // `validate`: run the soundness check on the built value; a failure is a
        // `BuilderError::Invalid`. The parser stays permissive (decode never runs it).
        let post_build = args.validate.as_ref().map(|path| {
            quote! {
                (#path)(&__value)
                    .map_err(|__e| #bnb::BuilderError::invalid(__e.to_string()))?;
            }
        });
        let mut bfields = Vec::new();
        for f in &full_fields.named {
            if field_is_temp(f) {
                continue; // a temp field is not stored, so not a builder field
            }
            let ident = f.ident.clone().expect("named field");
            let ty = f.ty.clone();
            // A reserved field is optional, defaulting to its spec value (so the builder
            // doesn't require it, but a caller can override it). A normal field is
            // required unless it carries `#[builder(default[= …])]`.
            let mut default = match reserved_spec_value(f)? {
                Some(spec) => crate::builder::FieldDefault::DefaultExpr(syn::parse2(spec)?),
                None => crate::builder::FieldDefault::Required,
            };
            for attr in &f.attrs {
                if let Some(d) = crate::builder::parse_builder_attr(attr)? {
                    default = d;
                }
            }
            bfields.push(crate::builder::BField { ident, ty, default });
        }
        // The injected `encode_mode` is an optional builder field (defaults to `Verbatim`),
        // so `build()` also initializes it — exposing `.encode_mode(…)` at construction.
        if inject_mode {
            bfields.push(crate::builder::BField {
                ident: syn::parse_quote!(encode_mode),
                ty: syn::parse_quote!(#bnb::EncodeMode),
                default: crate::builder::FieldDefault::DefaultExpr(
                    syn::parse_quote!(#bnb::EncodeMode::Verbatim),
                ),
            });
        }
        crate::builder::generate(
            &s.ident,
            &s.vis,
            &bfields,
            crate::builder::BuildKind::Plain,
            post_build.as_ref(),
        )
    };

    // `ctx(...)`: the single front-end owns the generated `<Name>Ctx` struct.
    let ctx_struct = if args.ctx.is_empty() {
        quote!()
    } else {
        let ctx_name = ctx_struct_ident(&s.ident);
        let vis = &s.vis;
        let decls = args.ctx.iter().map(|(n, t)| quote!(#vis #n: #t));
        let params = args.ctx.iter().map(|(n, t)| quote!(#n: #t));
        let names = args.ctx.iter().map(|(n, _)| n);
        quote! {
            #[derive(Clone)]
            #[doc = "Context for the matching `#[bin(ctx(...))]` type — pass it to `decode_with`."]
            #vis struct #ctx_name { #(#decls),* }
            impl #ctx_name {
                #[doc = "Construct the context positionally, in declaration order."]
                #vis fn new(#(#params),*) -> Self {
                    Self { #(#names),* }
                }
            }
        }
    };

    Ok(quote! {
        #ctx_struct
        #clean
        #mode_extras
        #new_ctor
        #builder
        #decode
        #encode
    })
}

// ---------------------------------------------------------------------------
// `#[bin]` on an enum — a dispatched tagged union.
//
// A variant is selected by its on-wire `magic` (a constant read+written), by a read-only
// `tag` selector drawn from a `ctx` param (never on the wire), or a hybrid of the two;
// each variant is a mini-struct whose fields reuse the `#[br]`/`#[bw]` grammar.
// `#[catch_all]` preserves an unknown discriminant and its payload (dual-use). Decode
// reuses `field_read_stmt` (it reads into locals, so it is variant-agnostic); encode needs
// a local-based writer because the struct writer is `self.#id`-coupled.
// ---------------------------------------------------------------------------

/// The bind idents for a variant's fields — the field's own ident (named) or a
/// synthesized `__f{i}` (tuple). Empty for a unit variant.
fn variant_bind_idents(fields: &Fields) -> Vec<Ident> {
    fields
        .iter()
        .enumerate()
        .map(|(i, f)| f.ident.clone().unwrap_or_else(|| format_ident!("__f{}", i)))
        .collect()
}

/// `Name::Variant { a, b }` / `Name::Variant(a, b)` / `Name::Variant` — serves as
/// **both** the destructuring pattern (encode/`tag`) and the construction expr
/// (decode), which are syntactically identical with field-shorthand.
fn variant_path_fields(
    name: &Ident,
    vid: &Ident,
    fields: &Fields,
    idents: &[Ident],
) -> TokenStream2 {
    match fields {
        Fields::Named(_) => quote!(#name::#vid { #(#idents),* }),
        Fields::Unnamed(_) => quote!(#name::#vid( #(#idents),* )),
        Fields::Unit => quote!(#name::#vid),
    }
}

/// `ctx { … }` literal for a **variant** encode arm. A name that is a stored sibling is
/// a match-bound `&FieldTy`, so it is dereferenced (`*n`); a `temp` local or an enum
/// `ctx` parameter is already a value (`n`). (The struct dual, [`ctx_literal`], uses
/// `self.n` for stored fields instead.)
fn ctx_literal_variant(ctx_ty: &TokenStream2, names: &[Ident], stored: &[Ident]) -> TokenStream2 {
    let inits = names.iter().map(|n| {
        if stored.contains(n) {
            quote!(#n: *#n)
        } else {
            quote!(#n)
        }
    });
    quote!(#ctx_ty { #(#inits),* })
}

/// The encode statement for one variant field, addressing the **match-bound local**
/// `id` (a `&FieldTy`) rather than `self.#id`. Mirrors [`field_write_core`] for the
/// variant world: `calc`/`temp`/`ctx` resolve sibling names against `stored` (the
/// arm's bound stored fields), which a variant `ctx` literal dereferences.
fn variant_field_write(
    f: &syn::Field,
    br: &FieldBr,
    id: &Ident,
    stored: &[Ident],
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let ty = &f.ty;
    let pre = pad_write_tokens(br.align_before, br.pad_before.as_ref());
    let post = pad_write_tokens(br.align_after, br.pad_after.as_ref());
    // `restore_position`: a read-side peek — the overlapping field owns the bytes.
    if br.restore_position {
        return Ok(quote!(#pre #post));
    }
    // `calc`: write a computed value. A `temp` field isn't in the match pattern, so bind
    // its value to a **named** local (so a later `ctx`/field can resolve it); a non-temp
    // `calc` field is in the pattern, so use a throwaway. The expr sees stored siblings as
    // references (use `s.len()` / `*s`), like a struct `calc` sees `self.s`.
    if let Some(calc) = &br.calc {
        let core = if br.temp {
            quote! {
                let #id: #ty = #calc;
                #bnb::__private::Sink::write(__bnb_w, #id)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            }
        } else {
            quote! {{
                let __v: #ty = #calc;
                #bnb::__private::Sink::write(__bnb_w, __v)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            }}
        };
        return Ok(quote!(#pre #core #post));
    }
    if br.temp {
        return Err(syn::Error::new_spanned(
            f,
            "a `#[br(temp)]` variant field is not stored, so it needs `#[bw(calc = <expr>)]` to encode",
        ));
    }
    // `ctx { … }` on a single nested ctx-message (the `Vec<_>` case is handled below):
    // resolve the passed names against the arm's stored siblings (deref'd) and enum ctx
    // params / temp locals (by value).
    if let (Some(names), None) = (&br.ctx, vec_elem(f)) {
        let child_ctx = ctx_struct_ty(ty)?;
        let lit = ctx_literal_variant(&child_ctx, names, stored);
        let core = quote!(<#ty as #bnb::EncodeWith<#child_ctx>>::encode_with(#id, __bnb_w, #lit)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;);
        return Ok(quote!(#pre #core #post));
    }
    let core = if br.ignore {
        quote!()
    } else if let Some(bw_map) = &br.bw_map {
        quote!(#bnb::__private::write_mapped(__bnb_w, #id, #bw_map)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
    } else if let Some(wf) = &br.write_with {
        quote!((#wf)(#id, __bnb_w).map_err(|e| e.in_field(::core::stringify!(#id)))?;)
    } else if br.map.is_some() || br.try_map.is_some() {
        return Err(syn::Error::new_spanned(
            f,
            "a `#[br(map = …)]`/`#[br(try_map = …)]` variant field needs the inverse `#[bw(map = <f>)]`",
        ));
    } else if br.parse_with.is_some() {
        return Err(syn::Error::new_spanned(
            f,
            "a `#[br(parse_with = …)]` variant field needs the inverse `#[bw(write_with = <f>)]`",
        ));
    } else if br.cond.is_some() {
        let inner = option_elem(f).ok_or_else(|| {
            syn::Error::new_spanned(f, "`#[br(if(...))]` requires an `Option<_>`")
        })?;
        let write_inner = if is_nested(f) {
            quote!(<#inner as #bnb::__private::BitEncode>::bit_encode(__v, __bnb_w)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else {
            quote!(#bnb::__private::Sink::write(__bnb_w, *__v)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        };
        quote!(if let ::core::option::Option::Some(__v) = #id { #write_inner })
    } else if let Some(elem) = vec_elem(f) {
        let write_elem = if let Some(names) = &br.ctx {
            let elem_ctx = ctx_struct_ty(elem)?;
            let lit = ctx_literal_variant(&elem_ctx, names, stored);
            quote!(<#elem as #bnb::EncodeWith<#elem_ctx>>::encode_with(__e, __bnb_w, #lit)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else if is_nested(f) {
            quote!(<#elem as #bnb::__private::BitEncode>::bit_encode(__e, __bnb_w)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else {
            quote!(#bnb::__private::Sink::write(__bnb_w, *__e)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        };
        quote!(for __e in #id { #write_elem })
    } else if is_nested(f) {
        quote!(<#ty as #bnb::__private::BitEncode>::bit_encode(#id, __bnb_w)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
    } else if byte_array_len(f).is_some() {
        quote!(#bnb::__private::write_byte_array(#id, __bnb_w)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
    } else {
        quote!(#bnb::__private::Sink::write(__bnb_w, *#id)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
    };
    Ok(quote!(#pre #core #post))
}

// ---------------------------------------------------------------------------
// Dispatch model for `#[bin]` enums. Two orthogonal axes:
//   * `tag`   — a read-only selector from `ctx` (never on the wire) that *picks*
//               the variant; takes priority over magic.
//   * `magic` — a wire constant (byte string or byte-aligned unsigned int literal),
//               verified on read and written on encode; the discriminant when there
//               is no tag, or a post-selection signature when there is.
// This module is the parsed+validated model; `bin_enum` (below) is wired onto it.
// ---------------------------------------------------------------------------

/// A `magic` wire constant. Restricted to **byte-oriented literals** so its on-wire
/// width is unambiguous: a byte string (`b"IHDR"`) or a width-suffixed unsigned
/// integer (`0x01u16`). Sub-byte types (`u4`) and non-literals are rejected.
enum Magic {
    /// A byte-string/byte literal — its raw bytes.
    Bytes(Vec<u8>),
    /// A byte-aligned unsigned integer literal — the expression and its byte width.
    /// `value` is boxed: `syn::Expr` is large (~240 bytes), so an unboxed variant would
    /// dominate the enum's size (`clippy::large_enum_variant`).
    Int { value: Box<syn::Expr>, width: usize },
}

/// The unsigned integer type token for a byte width (1/2/4/8/16).
fn int_type_for_width(width: usize) -> TokenStream2 {
    match width {
        1 => quote!(u8),
        2 => quote!(u16),
        4 => quote!(u32),
        8 => quote!(u64),
        16 => quote!(u128),
        _ => unreachable!("magic int width is validated to 1/2/4/8/16"),
    }
}

impl Magic {
    /// The on-wire byte length of this magic.
    fn byte_len(&self) -> usize {
        match self {
            Magic::Bytes(b) => b.len(),
            Magic::Int { width, .. } => *width,
        }
    }

    /// A coarse shape discriminator (byte-string vs integer) for "all magics in this
    /// enum read the same way" checks — two magics dispatch uniformly iff their
    /// `(kind, byte_len)` agree.
    fn kind(&self) -> u8 {
        match self {
            Magic::Bytes(_) => 0,
            Magic::Int { .. } => 1,
        }
    }

    /// The type this magic is read into: `[u8; N]` for a byte string, the unsigned
    /// integer type for an int.
    fn read_type(&self) -> TokenStream2 {
        match self {
            Magic::Bytes(b) => {
                let n = b.len();
                quote!([u8; #n])
            }
            Magic::Int { width, .. } => int_type_for_width(*width),
        }
    }

    /// A `read_type`-valued constant equal to this magic — for `==` dispatch and for
    /// writing a known variant's magic.
    fn const_expr(&self) -> TokenStream2 {
        match self {
            Magic::Bytes(b) => {
                let bytes = b.iter();
                quote!([#(#bytes),*])
            }
            Magic::Int { value, .. } => quote!(#value),
        }
    }

    /// Read this magic from `r` into the local `binding`.
    fn read_into(&self, binding: &Ident) -> TokenStream2 {
        let bnb = crate::bnb_path();
        let ty = self.read_type();
        match self {
            Magic::Bytes(_) => quote!(
                let #binding: #ty = #bnb::__private::read_byte_array(__bnb_r).map_err(|e| e.in_field("magic"))?;
            ),
            Magic::Int { .. } => quote!(
                let #binding: #ty = #bnb::__private::Source::read(__bnb_r).map_err(|e| e.in_field("magic"))?;
            ),
        }
    }

    /// Read and verify this magic, erroring on mismatch (`what` names the site).
    fn verify(&self, what: &str) -> TokenStream2 {
        let bnb = crate::bnb_path();
        let binding = format_ident!("__vm");
        let read = self.read_into(&binding);
        let expected = self.const_expr();
        let msg = format!("magic mismatch ({what})");
        quote! {
            #read
            if #binding != #expected {
                return ::core::result::Result::Err(
                    #bnb::__private::BitError::convert(
                        #bnb::__private::String::from(#msg),
                        #bnb::__private::Source::bit_pos(__bnb_r),
                    ).in_field("magic"),
                );
            }
        }
    }

    /// Write `value` (a `read_type`-typed expression) as this magic to `w`.
    fn write_value(&self, value: &TokenStream2) -> TokenStream2 {
        let bnb = crate::bnb_path();
        match self {
            Magic::Bytes(_) => quote!(
                #bnb::__private::write_byte_array(&#value, __bnb_w).map_err(|e| e.in_field("magic"))?;
            ),
            Magic::Int { .. } => quote!(
                #bnb::__private::Sink::write(__bnb_w, #value).map_err(|e| e.in_field("magic"))?;
            ),
        }
    }

    /// Write this magic's constant value to `w` (for a known variant).
    fn write_const(&self) -> TokenStream2 {
        let c = self.const_expr();
        self.write_value(&c)
    }
}

/// Parses + validates a `magic = <literal>` value into a [`Magic`].
fn parse_magic(expr: &syn::Expr) -> syn::Result<Magic> {
    if let syn::Expr::Lit(syn::ExprLit { lit, .. }) = expr {
        match lit {
            syn::Lit::ByteStr(s) => return Ok(Magic::Bytes(s.value())),
            syn::Lit::Byte(b) => return Ok(Magic::Bytes(vec![b.value()])),
            syn::Lit::Int(li) => {
                let width = match li.suffix() {
                    "u8" => 1usize,
                    "u16" => 2,
                    "u32" => 4,
                    "u64" => 8,
                    "u128" => 16,
                    "" => {
                        return Err(syn::Error::new_spanned(
                            expr,
                            "a `magic` integer needs a width suffix so its wire size is unambiguous, e.g. `0x01u16`",
                        ));
                    }
                    other => {
                        return Err(syn::Error::new_spanned(
                            expr,
                            format!(
                                "`{other}` is not a valid `magic` type; use a byte-aligned unsigned integer (u8/u16/u32/u64/u128) or a byte string"
                            ),
                        ));
                    }
                };
                return Ok(Magic::Int {
                    value: Box::new(expr.clone()),
                    width,
                });
            }
            _ => {}
        }
    }
    Err(syn::Error::new_spanned(
        expr,
        "a `magic` must be a byte string (`b\"…\"`) or a byte-aligned unsigned integer literal (`0x01u16`)",
    ))
}

/// How a single variant is selected.
#[derive(PartialEq, Eq, Debug)]
enum VariantRole {
    /// `#[bin(tag = V)]` — chosen by the selector; no wire signature.
    TagOnly,
    /// `#[bin(tag = V, magic = M)]` — chosen by the selector, then verify `M`.
    TagAndMagic,
    /// `#[bin(magic = M)]` — chosen by matching `M` on the wire.
    MagicOnly,
    /// neither tag nor magic — the typed fallback (at most one).
    Fallback,
    /// `#[catch_all]` — the raw capture (at most one).
    CatchAll,
}

/// One variant's dispatch directives.
struct VariantDispatch<'a> {
    variant: &'a syn::Variant,
    /// `#[bin(tag = V)]` — the selector value to match against.
    tag: Option<syn::Expr>,
    /// `#[bin(magic = M)]` — the wire signature.
    magic: Option<Magic>,
    catch_all: bool,
}

impl VariantDispatch<'_> {
    fn role(&self) -> VariantRole {
        if self.catch_all {
            VariantRole::CatchAll
        } else {
            match (self.tag.is_some(), self.magic.is_some()) {
                (true, true) => VariantRole::TagAndMagic,
                (true, false) => VariantRole::TagOnly,
                (false, true) => VariantRole::MagicOnly,
                (false, false) => VariantRole::Fallback,
            }
        }
    }
}

/// The uniformity of the variant magic widths — decides single-read vs peek dispatch.
#[derive(PartialEq, Eq, Debug)]
enum MagicWidth {
    /// No variant carries a magic.
    None,
    /// Every magic-bearing variant has this same byte width (single-read dispatch).
    Uniform(usize),
    /// Magics differ in width (peek-and-match dispatch — a later step).
    Mixed,
}

/// The parsed + validated dispatch plan for a `#[bin]` enum.
struct EnumDispatch<'a> {
    /// `#[bin(tag = <ctx-param>)]` — the selector for tag-variants, if any.
    selector: Option<Ident>,
    /// `#[bin(magic = <const>)]` — an optional leading prefix.
    prefix: Option<Magic>,
    variants: Vec<VariantDispatch<'a>>,
}

impl<'a> EnumDispatch<'a> {
    /// Parses every variant's dispatch directives and validates the structural rules
    /// (a tag-variant needs a declared selector; at most one `#[catch_all]`; at most
    /// one typed fallback). `selector`/`prefix` come from the enum-level `#[bin(...)]`.
    fn parse(
        e: &'a syn::ItemEnum,
        selector: Option<Ident>,
        prefix: Option<Magic>,
    ) -> syn::Result<Self> {
        let mut variants = Vec::new();
        let mut catch_alls = 0u32;
        let mut fallbacks = 0u32;
        for v in &e.variants {
            let mut tag = None;
            let mut magic = None;
            let mut catch_all = false;
            for a in &v.attrs {
                if a.path().is_ident("catch_all") {
                    catch_all = true;
                } else if a.path().is_ident("bin") {
                    a.parse_nested_meta(|m| {
                        if m.path.is_ident("tag") {
                            tag = Some(m.value()?.parse()?);
                            Ok(())
                        } else if m.path.is_ident("magic") {
                            let expr: syn::Expr = m.value()?.parse()?;
                            magic = Some(parse_magic(&expr)?);
                            Ok(())
                        } else {
                            Err(m.error(
                                "expected `tag = <value>` or `magic = <literal>` on a variant",
                            ))
                        }
                    })?;
                }
            }
            let vd = VariantDispatch {
                variant: v,
                tag,
                magic,
                catch_all,
            };
            match vd.role() {
                VariantRole::CatchAll => catch_alls += 1,
                VariantRole::Fallback => fallbacks += 1,
                VariantRole::TagOnly | VariantRole::TagAndMagic if selector.is_none() => {
                    return Err(syn::Error::new_spanned(
                        &v.ident,
                        "a variant with `tag = …` needs the enum to declare the selector via `#[bin(tag = <ctx-param>)]`",
                    ));
                }
                _ => {}
            }
            variants.push(vd);
        }
        if catch_alls > 1 {
            return Err(syn::Error::new_spanned(
                &e.ident,
                "a `#[bin]` enum may have at most one `#[catch_all]` variant",
            ));
        }
        if fallbacks > 1 {
            return Err(syn::Error::new_spanned(
                &e.ident,
                "a `#[bin]` enum may have at most one no-tag/no-magic fallback variant",
            ));
        }
        Ok(EnumDispatch {
            selector,
            prefix,
            variants,
        })
    }

    /// The width uniformity of the **dispatching** magics (magic-only variants). A magic
    /// on a tag-variant is a post-selection signature, verified per variant, so it never
    /// participates in the read-once-then-match decision this drives.
    fn magic_width(&self) -> MagicWidth {
        let mut width: Option<usize> = None;
        let mut mixed = false;
        for v in &self.variants {
            if v.role() == VariantRole::MagicOnly {
                let len = v.magic.as_ref().expect("MagicOnly has a magic").byte_len();
                match width {
                    None => width = Some(len),
                    Some(__bnb_w) if __bnb_w != len => mixed = true,
                    _ => {}
                }
            }
        }
        match (width, mixed) {
            (_, true) => MagicWidth::Mixed,
            (Some(__bnb_w), false) => MagicWidth::Uniform(__bnb_w),
            (None, false) => MagicWidth::None,
        }
    }

    /// The read type shared by all dispatching magics, if they agree on `(kind, width)`
    /// (a single-read `__m`); `None` if there are none or they disagree (peek dispatch).
    fn uniform_magic_read_type(&self) -> Option<TokenStream2> {
        let mut shape: Option<(u8, usize)> = None;
        let mut ty = None;
        for v in &self.variants {
            if v.role() == VariantRole::MagicOnly {
                let m = v.magic.as_ref().expect("MagicOnly has a magic");
                let s = (m.kind(), m.byte_len());
                match shape {
                    None => {
                        shape = Some(s);
                        ty = Some(m.read_type());
                    }
                    Some(prev) if prev != s => return None,
                    _ => {}
                }
            }
        }
        ty
    }
}

/// Field-level decode/encode for one variant: the per-field reads + the path-with-fields
/// (used as both the decode constructor and the encode pattern) + the per-field writes.
/// For a `#[catch_all]`, `catch_capture` is the discriminant expression bound into the
/// first field on read; that field is omitted from `writes` (the dispatch emits it).
/// `#[br(temp)]` fields are read but dropped from the variant (mirrors a struct).
fn variant_field_codec(
    name: &Ident,
    v: &syn::Variant,
    catch_capture: Option<&TokenStream2>,
) -> syn::Result<(Vec<TokenStream2>, TokenStream2, Vec<TokenStream2>)> {
    let vid = &v.ident;
    let idents = variant_bind_idents(&v.fields);
    let stored_idents: Vec<Ident> = v
        .fields
        .iter()
        .zip(&idents)
        .filter(|(f, _)| !field_is_temp(f))
        .map(|(_, id)| id.clone())
        .collect();
    let path = variant_path_fields(name, vid, &v.fields, &stored_idents);

    let is_catch = catch_capture.is_some();
    if is_catch && v.fields.is_empty() {
        return Err(syn::Error::new_spanned(
            vid,
            "a `#[catch_all]` variant needs a first field to hold the captured discriminant",
        ));
    }

    let mut reads = Vec::new();
    for (i, f) in v.fields.iter().enumerate() {
        let id = &idents[i];
        let br = parse_field_br(f)?;
        if is_catch && i == 0 {
            if br.temp {
                return Err(syn::Error::new_spanned(
                    f,
                    "the `#[catch_all]` first field holds the captured discriminant, so it can't be `#[br(temp)]`",
                ));
            }
            let cap = catch_capture.expect("catch capture present");
            reads.push(quote!(let #id = #cap;));
        } else {
            let mut nf = f.clone();
            nf.ident = Some(id.clone());
            reads.push(field_read_stmt(&nf, &br)?);
        }
    }

    let mut writes = Vec::new();
    for (i, f) in v.fields.iter().enumerate() {
        if is_catch && i == 0 {
            continue; // the captured discriminant — the dispatch emits it
        }
        let id = &idents[i];
        let br = parse_field_br(f)?;
        writes.push(variant_field_write(f, &br, id, &stored_idents)?);
    }

    Ok((reads, path, writes))
}

/// `CamelCase` → `snake_case`, for the generated `decode_as_<variant>` methods.
fn snake_case(ident: &Ident) -> String {
    let s = ident.to_string();
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.char_indices() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// The `#[bin]` enum path. See the module banner above.
fn bin_enum(args: &BinArgs, e: &syn::ItemEnum) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let name = &e.ident;
    let vis = &e.vis;
    if !e.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &e.generics,
            "#[bin] does not support generic parameters yet",
        ));
    }
    if args.validate.is_some() {
        return Err(syn::Error::new_spanned(
            name,
            "`validate` needs the builder; a `#[bin]` enum has none",
        ));
    }

    // Enum-level `magic` is a leading prefix constant, verified on read / written on
    // encode before dispatch. Then parse + validate the per-variant dispatch model.
    let prefix = args.magic.as_ref().map(parse_magic).transpose()?;
    let dispatch = EnumDispatch::parse(e, args.tag.clone(), prefix)?;

    // Dispatch mode + the variants that form the "nothing matched" tail.
    let has_selector = dispatch.selector.is_some();
    let magic_dispatch = dispatch
        .variants
        .iter()
        .any(|v| v.role() == VariantRole::MagicOnly);
    let fallback_variant = dispatch
        .variants
        .iter()
        .find(|v| v.role() == VariantRole::Fallback);
    let catch_variant = dispatch
        .variants
        .iter()
        .find(|v| v.role() == VariantRole::CatchAll);

    // Hybrid (some tag variants, some magic-only): the selector picks a tag variant first,
    // then unmatched selectors fall through to magic dispatch (tag priority).
    let hybrid = has_selector && magic_dispatch;
    if !has_selector && !magic_dispatch {
        return Err(syn::Error::new_spanned(
            name,
            "a `#[bin]` enum dispatches on `tag = <ctx-param>` (variant `tag`s) or per-variant `magic`s; it has neither",
        ));
    }
    if fallback_variant.is_some() && catch_variant.is_some() {
        return Err(syn::Error::new_spanned(
            name,
            "use either a typed fallback variant (no tag/magic) or a `#[catch_all]`, not both",
        ));
    }

    // Magic dispatch reads the discriminant once and matches by `==` when the magics are
    // uniform-width and there is no typed fallback; otherwise it **peeks** the longest
    // magic, matches a prefix, and seeks past the winner — so a fallback / catch-all can
    // read the still-unconsumed bytes. The peek path needs byte-string magics (so
    // `starts_with` is well-defined) and a seekable source.
    let mixed = magic_dispatch && dispatch.magic_width() == MagicWidth::Mixed;
    let use_peek = magic_dispatch && (mixed || fallback_variant.is_some());
    if use_peek
        && dispatch.variants.iter().any(|v| {
            v.role() == VariantRole::MagicOnly
                && matches!(v.magic.as_ref().expect("magic-only"), Magic::Int { .. })
        })
    {
        return Err(syn::Error::new_spanned(
            name,
            "variable-width / fallback magic dispatch needs byte-string magics (so an unmatched discriminant can be re-read)",
        ));
    }

    // For tag dispatch: the selector ident + its `ctx` type. For magic dispatch: a
    // representative dispatching magic (drives the single-read `__m` + the catch writer).
    let selector_ty = dispatch
        .selector
        .as_ref()
        .map(|sel| {
            args.ctx
                .iter()
                .find(|(n, _)| n == sel)
                .map(|(_, t)| t.clone())
                .ok_or_else(|| {
                    syn::Error::new_spanned(
                        sel,
                        "`tag = <ctx-param>` must name a `ctx(...)` parameter",
                    )
                })
        })
        .transpose()?;
    let rep_magic = dispatch
        .variants
        .iter()
        .find_map(|v| (v.role() == VariantRole::MagicOnly).then(|| v.magic.as_ref().unwrap()));

    // The "nothing matched" tail: a typed fallback (parse the unconsumed bytes — no
    // capture), else a `#[catch_all]` capturing the read magic (`__m`, single-read), the
    // unmatched selector (`__other`, tag), or nothing (peek path — the magic stays in the
    // catch-all's own fields).
    let tail_variant = fallback_variant.or(catch_variant);
    let tail_capture: Option<TokenStream2> = if fallback_variant.is_some() {
        None
    } else if catch_variant.is_some() {
        if use_peek {
            None
        } else if magic_dispatch {
            Some(quote!(__m))
        } else {
            Some(quote!(__other))
        }
    } else {
        None
    };

    // An accessor (`tag()`/`magic()`) reports a single discriminant; that is only
    // well-defined for a uniform-width, single-kind dispatch with no typed fallback (so
    // not mixed-width, not a fallback, and not a hybrid's two discriminant kinds).
    let gen_accessor = !mixed && fallback_variant.is_none() && !hybrid;

    // Per-variant encode arms (+ accessor arms). The tail variant (typed fallback or
    // catch-all) writes no discriminant of its own, except a single-read catch-all, which
    // writes back its captured magic.
    let mut encode_arms = Vec::new();
    let mut accessor_arms = Vec::new();
    for v in &dispatch.variants {
        let is_tail = matches!(v.role(), VariantRole::Fallback | VariantRole::CatchAll);
        let cap = if is_tail { tail_capture.clone() } else { None };
        let (_, pat, writes) = variant_field_codec(name, v.variant, cap.as_ref())?;

        let write_disc = if v.role() == VariantRole::CatchAll && cap.is_some() && magic_dispatch {
            let first = &variant_bind_idents(&v.variant.fields)[0];
            match rep_magic.expect("magic dispatch").kind() {
                0 => {
                    quote!(#bnb::__private::write_byte_array(#first, __bnb_w).map_err(|e| e.in_field("magic"))?;)
                }
                _ => {
                    quote!(#bnb::__private::Sink::write(__bnb_w, *#first).map_err(|e| e.in_field("magic"))?;)
                }
            }
        } else if let Some(m) = v.magic.as_ref().filter(|_| !is_tail) {
            m.write_const()
        } else {
            quote!()
        };
        encode_arms.push(quote!(#pat => { #write_disc #(#writes)* }));

        if gen_accessor {
            let acc = if is_tail {
                let first = &variant_bind_idents(&v.variant.fields)[0];
                quote!(*#first)
            } else if magic_dispatch {
                v.magic.as_ref().expect("magic variant").const_expr()
            } else {
                let tagval = v.tag.as_ref().expect("tag variant");
                quote!(#tagval)
            };
            accessor_arms.push(quote!(#pat => #acc,));
        }
    }

    // The "nothing matched" body: the tail variant (a typed fallback parsing the
    // unconsumed bytes, or a catch-all capturing the discriminant), else — a closed set —
    // an `unrecognized discriminant` error.
    let disc_field = if magic_dispatch { "magic" } else { "tag" };
    let tail_body = if let Some(tv) = tail_variant {
        let (reads, ctor, _) = variant_field_codec(name, tv.variant, tail_capture.as_ref())?;
        quote! {{
            #(#reads)*
            ::core::result::Result::Ok(#ctor)
        }}
    } else {
        quote! {{
            ::core::result::Result::Err(#bnb::__private::BitError::convert(
                #bnb::__private::String::from(concat!("unrecognized ", stringify!(#name), " discriminant")),
                #bnb::__private::Source::bit_pos(__bnb_r),
            ).in_field(#disc_field))
        }}
    };

    // The decode dispatch. Magic: a single `__m` read + an `==` chain (uniform width, no
    // fallback), or a `peek_bytes` + `starts_with` + seek chain (variable width / fallback,
    // so the tail can re-read the unconsumed magic). Tag: a `match` on the selector. The
    // tail body is the final else.
    //
    // The magic block: a single `__m` read + `==` chain (uniform width), or a `peek_bytes`
    // + `starts_with` + seek chain (variable width / fallback), ending in the tail body.
    // Used directly for pure-magic dispatch, and as the selector `match`'s fall-through
    // under hybrid (tag takes priority, then magic).
    let magic_block = if !magic_dispatch {
        quote!()
    } else if use_peek {
        let max = dispatch
            .variants
            .iter()
            .filter(|v| v.role() == VariantRole::MagicOnly)
            .map(|v| v.magic.as_ref().unwrap().byte_len())
            .max()
            .expect("at least one magic-only variant");
        let mut chain = tail_body.clone();
        for v in dispatch.variants.iter().rev() {
            if v.role() == VariantRole::MagicOnly {
                let Magic::Bytes(bytes) = v.magic.as_ref().unwrap() else {
                    unreachable!("peek path validated to byte-string magics");
                };
                let len = bytes.len();
                let bytes = bytes.iter();
                let (reads, ctor, _) = variant_field_codec(name, v.variant, None)?;
                chain = quote! {
                    if __peek.starts_with(&[#(#bytes),*]) {
                        #bnb::__private::Source::seek_to_bit(__bnb_r, #bnb::__private::Source::bit_pos(__bnb_r) + #len * 8)?;
                        #(#reads)*
                        ::core::result::Result::Ok(#ctor)
                    } else #chain
                };
            }
        }
        quote! {
            let __peek = #bnb::__private::peek_bytes(__bnb_r, #max)?;
            #chain
        }
    } else {
        let read = rep_magic
            .expect("magic dispatch has a representative magic")
            .read_into(&format_ident!("__m"));
        let mut chain = tail_body.clone();
        for v in dispatch.variants.iter().rev() {
            if v.role() == VariantRole::MagicOnly {
                let c = v.magic.as_ref().unwrap().const_expr();
                let (reads, ctor, _) = variant_field_codec(name, v.variant, None)?;
                chain = quote! {
                    if __m == #c {
                        #(#reads)*
                        ::core::result::Result::Ok(#ctor)
                    } else #chain
                };
            }
        }
        quote!(#read #chain)
    };

    let dispatch_decode = if has_selector {
        // Tag dispatch (incl. hybrid): match the selector; an unmatched selector falls to
        // the magic block (hybrid) or the tail body (pure tag).
        let sel = dispatch
            .selector
            .as_ref()
            .expect("tag dispatch has a selector");
        let mut arms = Vec::new();
        for v in &dispatch.variants {
            if matches!(v.role(), VariantRole::TagOnly | VariantRole::TagAndMagic) {
                let tagval = v.tag.as_ref().unwrap();
                let verify = v
                    .magic
                    .as_ref()
                    .map(|m| m.verify(&v.variant.ident.to_string()));
                let (reads, ctor, _) = variant_field_codec(name, v.variant, None)?;
                arms.push(quote! {
                    #tagval => {
                        #verify
                        #(#reads)*
                        ::core::result::Result::Ok(#ctor)
                    }
                });
            }
        }
        let else_body = if magic_dispatch {
            quote!({ #magic_block })
        } else {
            tail_body.clone()
        };
        // A pure-tag catch-all captures the unmatched selector (`__other`); a hybrid reads
        // `__m` in the magic block, so the unmatched selector is unused there.
        let catch_pat = if !magic_dispatch && tail_variant.is_some() {
            quote!(__other)
        } else {
            quote!(_)
        };
        quote!(match #sel { #(#arms)* #catch_pat => #else_body })
    } else {
        magic_block
    };

    let prefix_verify = dispatch.prefix.as_ref().map(|m| m.verify("prefix"));
    let prefix_write = dispatch.prefix.as_ref().map(|m| m.write_const());

    let decode_body = quote! {
        #prefix_verify
        #dispatch_decode
    };
    let encode_body = quote! {
        #prefix_write
        match self {
            #(#encode_arms)*
        }
        ::core::result::Result::Ok(())
    };

    // The explicit source is bound on `SeekSource` when a variant field seeks
    // (`seek`/`restore_position`) or the variable-width / fallback magic path peeks; a
    // `forward_only` enum then rejects either, mirroring the struct path.
    let seeks = use_peek
        || e.variants
            .iter()
            .flat_map(|v| &v.fields)
            .any(|f| parse_field_br(f).is_ok_and(|br| br.restore_position || br.seek.is_some()));
    if args.forward_only && seeks {
        let reason = if use_peek {
            "variable-width / fallback magic dispatch peeks (it needs to seek)"
        } else {
            "a seeking variant field (`seek`/`restore_position`)"
        };
        return Err(syn::Error::new_spanned(
            name,
            format!("{reason} is incompatible with `#[bin(forward_only)]`"),
        ));
    }
    let from_bound = if seeks {
        quote!(#bnb::__private::SeekSource)
    } else {
        quote!(#bnb::__private::Source)
    };

    let attrs = BitStreamAttrs {
        allow_byte_aligned: true,
        lsb: args.lsb,
        little: args.little,
        magic: None,
        ctx: args.ctx.clone(),
    };
    let layout = layout_token(&attrs);
    let want_decode = !args.write_only;
    let want_encode = !args.read_only;
    let is_ctx_type = !args.ctx.is_empty();
    let ctx_binds: Vec<TokenStream2> = args
        .ctx
        .iter()
        .map(|(n, _)| quote!(let #n = __ctx.#n;))
        .collect();

    // An inherent accessor on a dispatched enum: `tag()` (the off-wire selector this value
    // dispatches as — drives a parent's `#[bw(calc = self.body.tag())]`) for tag dispatch,
    // or `magic()` (the wire signature) for magic dispatch. Omitted when there is no
    // single discriminant to report (variable-width magic, or a typed fallback).
    let accessor_fn = if !gen_accessor {
        quote!()
    } else if magic_dispatch {
        let magic_ty = dispatch
            .uniform_magic_read_type()
            .expect("uniform magic dispatch has a read type");
        quote! {
            impl #name {
                #[doc = "The wire magic this value encodes as."]
                #[allow(unused_variables)]
                pub fn magic(&self) -> #magic_ty {
                    match self {
                        #(#accessor_arms)*
                    }
                }
            }
        }
    } else {
        let sel_ty = selector_ty
            .as_ref()
            .expect("tag dispatch has a selector type");
        quote! {
            impl #name {
                #[doc = "The selector this value dispatches as (the off-wire `tag`)."]
                #[allow(unused_variables)]
                pub fn tag(&self) -> #sel_ty {
                    match self {
                        #(#accessor_arms)*
                    }
                }
            }
        }
    };

    // Decode helpers: `decode_as_<variant>` parses the bytes as one explicit variant
    // (its magic, if any, then its payload), bypassing dispatch — handy when the variant
    // is known out of band, and for tests. `decode_tagged` feeds a tag-dispatched enum
    // its selector directly. A `ctx` enum threads the context through both.
    let ctx_name = ctx_struct_ident(name);
    let mut helper_methods = Vec::new();
    for v in &dispatch.variants {
        if v.role() == VariantRole::CatchAll {
            continue; // the catch-all isn't an explicit target — use `decode`/`decode_with`
        }
        let mname = format_ident!("decode_as_{}", snake_case(&v.variant.ident));
        let (reads, ctor, _) = variant_field_codec(name, v.variant, None)?;
        let magic_verify = v
            .magic
            .as_ref()
            .map(|m| m.verify(&v.variant.ident.to_string()));
        let doc = format!(
            "Decode `bytes` as the `{}` variant — its `magic` (if any) then its payload, requiring every whole byte consumed.",
            v.variant.ident
        );
        let body = quote! {
            #prefix_verify
            #magic_verify
            #(#reads)*
            ::core::result::Result::Ok(#ctor)
        };
        if is_ctx_type {
            helper_methods.push(quote! {
                #[doc = #doc]
                #[allow(unused_variables)]
                pub fn #mname(bytes: &[u8], __ctx: #ctx_name) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_exact_with(bytes, #layout, |__bnb_r| { #(#ctx_binds)* #body })
                }
            });
        } else {
            helper_methods.push(quote! {
                #[doc = #doc]
                pub fn #mname(bytes: &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_exact_with(bytes, #layout, |__bnb_r| { #body })
                }
            });
        }
    }
    // `decode_tagged(selector, bytes)` — sugar for a tag-dispatched enum whose only
    // context is the selector.
    if !magic_dispatch && want_decode && args.ctx.len() == 1 {
        let sel = dispatch
            .selector
            .as_ref()
            .expect("tag dispatch has a selector");
        let sel_ty = selector_ty
            .as_ref()
            .expect("tag dispatch has a selector type");
        helper_methods.push(quote! {
            #[doc = "Decode `bytes` with the given selector (tag), then dispatch — sugar for `decode_with_exact`."]
            pub fn decode_tagged(#sel: #sel_ty, bytes: &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                Self::decode_with_exact(bytes, #ctx_name { #sel })
            }
        });
    }
    // `peek_variant` + a `<Name>Kind` enum: identify the variant by the wire magic
    // (the dispatch decision only, no payload). Magic dispatch only — under tag dispatch
    // the caller already holds the selector.
    let kind_name = format_ident!("{}Kind", name);
    let kind_enum = if magic_dispatch && !has_selector && want_decode {
        // The "nothing matched" kind: the tail variant's kind, else an error.
        let tail_kind = tail_variant
            .map(|tv| {
                let k = &tv.variant.ident;
                quote!(::core::result::Result::Ok(#kind_name::#k))
            })
            .unwrap_or_else(|| {
                quote!(::core::result::Result::Err(
                    #bnb::__private::BitError::convert(
                        #bnb::__private::String::from(concat!(
                            "unrecognized ",
                            stringify!(#name),
                            " discriminant"
                        )),
                        #bnb::__private::Source::bit_pos(__bnb_r),
                    )
                    .in_field("magic")
                ))
            });
        let decision = if use_peek {
            let max = dispatch
                .variants
                .iter()
                .filter(|v| v.role() == VariantRole::MagicOnly)
                .map(|v| v.magic.as_ref().unwrap().byte_len())
                .max()
                .expect("at least one magic-only variant");
            let mut chain = quote!({ #tail_kind });
            for v in dispatch.variants.iter().rev() {
                if v.role() == VariantRole::MagicOnly {
                    let Magic::Bytes(bytes) = v.magic.as_ref().unwrap() else {
                        unreachable!("peek path validated to byte-string magics");
                    };
                    let bytes = bytes.iter();
                    let k = &v.variant.ident;
                    chain = quote!(if __peek.starts_with(&[#(#bytes),*]) { ::core::result::Result::Ok(#kind_name::#k) } else #chain);
                }
            }
            quote! {
                let __peek = #bnb::__private::peek_bytes(__bnb_r, #max)?;
                #chain
            }
        } else {
            let read = rep_magic
                .expect("magic dispatch has a representative magic")
                .read_into(&format_ident!("__m"));
            let mut chain = quote!({ #tail_kind });
            for v in dispatch.variants.iter().rev() {
                if v.role() == VariantRole::MagicOnly {
                    let c = v.magic.as_ref().unwrap().const_expr();
                    let k = &v.variant.ident;
                    chain = quote!(if __m == #c { ::core::result::Result::Ok(#kind_name::#k) } else #chain);
                }
            }
            quote!(#read #chain)
        };
        helper_methods.push(quote! {
            #[doc = "Identify which variant `bytes` is from the wire magic, without parsing the payload."]
            pub fn peek_variant(bytes: &[u8]) -> ::core::result::Result<#kind_name, #bnb::__private::BitError> {
                #bnb::__private::decode_peek_with(bytes, #layout, |__bnb_r| {
                    #prefix_verify
                    #decision
                })
            }
        });
        let kvars = dispatch.variants.iter().map(|v| &v.variant.ident);
        quote! {
            #[doc = "The variant kind of a value, from `peek_variant` (the dispatch decision only)."]
            #[derive(::core::clone::Clone, ::core::marker::Copy, ::core::fmt::Debug, ::core::cmp::PartialEq, ::core::cmp::Eq)]
            #vis enum #kind_name { #(#kvars),* }
        }
    } else {
        quote!()
    };
    let helpers = if helper_methods.is_empty() {
        quote!()
    } else {
        quote!(impl #name { #(#helper_methods)* })
    };

    // The codec impls. Decode: a `ctx` enum gets `decode_with` (+ `DecodeWith`); a plain
    // one gets `BitDecode` + the slice/stream entry points. Encode: `ctx` is decode-only,
    // so a `ctx` enum still gets a **plain** `BitEncode`/`to_bytes` unless its encode body
    // actually reads a ctx param (a `calc`/`bw(map)`/`ctx`-forward naming one) — then it
    // gets `encode_with`/`to_bytes_with` instead. Either way it impls `EncodeWith` so a
    // parent can forward to it.
    let ctx_param_names: Vec<&Ident> = args.ctx.iter().map(|(n, _)| n).collect();
    let enum_encode_uses_ctx = is_ctx_type && tokens_mention(encode_body.clone(), &ctx_param_names);

    let decode = if !want_decode {
        quote!()
    } else if is_ctx_type {
        quote! {
            impl #name {
                #[doc = "Decode from a bit source, given the context this type declares via `ctx(...)`."]
                #[allow(unused_variables)]
                pub fn decode_with<S: #from_bound>(
                    __bnb_r: &mut S,
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #(#ctx_binds)*
                    #decode_body
                }
                #[doc = "Decode from bytes with context, requiring every whole byte consumed."]
                pub fn decode_with_exact(
                    bytes: &[u8],
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_exact_with(bytes, #layout, |__bnb_r| Self::decode_with(__bnb_r, __ctx.clone()))
                }
            }
            impl #bnb::DecodeWith<#ctx_name> for #name {
                fn decode_with<S: #bnb::__private::Source>(
                    __bnb_r: &mut S,
                    args: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    <#name>::decode_with(__bnb_r, args)
                }
            }
        }
    } else {
        quote! {
            impl #bnb::BitDecode for #name {
                fn bit_decode<S: #bnb::__private::Source>(
                    __bnb_r: &mut S,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #decode_body
                }
            }
            impl #name {
                #[doc = "Decode one message from the front of `buf`, advancing it past the bytes consumed."]
                pub fn decode(buf: &mut &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_consume(buf, #layout)
                }
                #[doc = "Decode one message from `bytes` without consuming the caller's buffer (tail-tolerant)."]
                pub fn peek(bytes: &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_peek(bytes, #layout)
                }
                #[doc = "Decode and require every whole byte consumed."]
                pub fn decode_exact(bytes: &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_exact(bytes, #layout)
                }
                #[doc = "Decode from an explicit bit source (a seekable one if a variant seeks)."]
                pub fn decode_from<S: #from_bound>(
                    __bnb_r: &mut S,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    <Self as #bnb::BitDecode>::bit_decode(__bnb_r)
                }
            }
        }
    };

    let encode = if !want_encode {
        quote!()
    } else if enum_encode_uses_ctx {
        quote! {
            impl #name {
                #[doc = "Encode to a bit sink, given the context this type declares via `ctx(...)`."]
                #[allow(unused_variables)]
                pub fn encode_with<K: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut K,
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    #(#ctx_binds)*
                    #encode_body
                }
                #[doc = "Encode to a `Vec<u8>` with context."]
                pub fn to_bytes_with(
                    &self,
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                    #bnb::__private::encode_to_vec_with(#layout, |__bnb_w| self.encode_with(__bnb_w, __ctx.clone()))
                }
            }
            impl #bnb::EncodeWith<#ctx_name> for #name {
                fn encode_with<K: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut K,
                    args: #ctx_name,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    <#name>::encode_with(self, __bnb_w, args)
                }
            }
        }
    } else {
        // Plain encode (ctx not used on the write side). A `ctx` enum additionally impls a
        // context-ignoring `EncodeWith` so a parent can forward to it uniformly.
        let encode_with_trait = is_ctx_type.then(|| {
            quote! {
                impl #bnb::EncodeWith<#ctx_name> for #name {
                    #[allow(unused_variables)]
                    fn encode_with<K: #bnb::__private::Sink>(
                        &self,
                        __bnb_w: &mut K,
                        args: #ctx_name,
                    ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                        <Self as #bnb::BitEncode>::bit_encode(self, __bnb_w)
                    }
                }
            }
        });
        quote! {
            impl #bnb::BitEncode for #name {
                const LAYOUT: #bnb::Layout = #layout;
                fn bit_encode<K: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut K,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    #encode_body
                }
            }
            impl #name {
                #[doc = "Encode to a `Vec<u8>`. To encode to a `std::io::Write` sink, bring"]
                #[doc = "[`EncodeExt`](::bnb::EncodeExt) into scope and call `.encode(&mut w)` (the `std` feature)."]
                pub fn to_bytes(&self) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                    #bnb::__private::encode_to_vec(self, #layout)
                }
                #[doc = "Encode into an explicit bit sink (a `BitWriter`)."]
                pub fn encode_into<K: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut K,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    <Self as #bnb::BitEncode>::bit_encode(self, __bnb_w)
                }
            }
            #encode_with_trait
        }
    };

    let ctx_struct = if is_ctx_type {
        let decls = args.ctx.iter().map(|(n, t)| quote!(#vis #n: #t));
        let params = args.ctx.iter().map(|(n, t)| quote!(#n: #t));
        let names = args.ctx.iter().map(|(n, _)| n);
        quote! {
            #[derive(Clone)]
            #[doc = "Context for the matching `#[bin(ctx(...))]` type — pass it to `decode_with`."]
            #vis struct #ctx_name { #(#decls),* }
            impl #ctx_name {
                #[doc = "Construct the context positionally, in declaration order."]
                #vis fn new(#(#params),*) -> Self {
                    Self { #(#names),* }
                }
            }
        }
    } else {
        quote!()
    };

    // The emitted enum: drop `#[br(temp)]` variant fields (read but not stored), strip
    // the dispatch attrs (`#[bin(tag=…)]`, `#[catch_all]`) and codec field attrs, leaving
    // an ordinary enum beside the generated impls.
    let strip = |f: &syn::Field| -> Option<syn::Field> {
        (!field_is_temp(f)).then(|| {
            let mut f = f.clone();
            f.attrs.retain(|a| !is_codec_field_attr(a));
            f
        })
    };
    let mut clean = e.clone();
    for v in &mut clean.variants {
        v.attrs
            .retain(|a| !(a.path().is_ident("bin") || a.path().is_ident("catch_all")));
        match &mut v.fields {
            Fields::Named(n) => n.named = n.named.iter().filter_map(&strip).collect(),
            Fields::Unnamed(u) => u.unnamed = u.unnamed.iter().filter_map(&strip).collect(),
            Fields::Unit => {}
        }
    }

    Ok(quote! {
        #ctx_struct
        #clean
        #kind_enum
        #accessor_fn
        #helpers
        #decode
        #encode
    })
}

#[cfg(test)]
mod dispatch_tests {
    // Source snippets are kept uniformly as `r#"…"#` (several contain `b"…"` magics).
    #![allow(clippy::needless_raw_string_hashes)]
    use super::{EnumDispatch, Magic, MagicWidth, VariantRole};

    fn enum_of(src: &str) -> syn::ItemEnum {
        syn::parse_str(src).expect("valid enum source")
    }
    fn sel(name: &str) -> syn::Ident {
        syn::parse_str(name).expect("valid ident")
    }

    #[test]
    fn integer_magic_widths_inferred_from_suffix() {
        let src = r#"enum E { #[bin(magic = 0xCAFEu16)] A(u32), #[bin(magic = 0x01u16)] B }"#;
        let e = enum_of(src);
        let d = EnumDispatch::parse(&e, None, None).unwrap();
        assert_eq!(d.variants.len(), 2);
        assert_eq!(d.variants[0].magic.as_ref().unwrap().byte_len(), 2);
        assert_eq!(d.variants[0].role(), VariantRole::MagicOnly);
        assert_eq!(d.magic_width(), MagicWidth::Uniform(2));
    }

    #[test]
    fn byte_string_magic_widths_and_mixed_detection() {
        let e = enum_of(r#"enum E { #[bin(magic = b"IHDR")] A, #[bin(magic = b"END")] B }"#);
        let d = EnumDispatch::parse(&e, None, None).unwrap();
        assert_eq!(d.variants[0].magic.as_ref().unwrap().byte_len(), 4);
        assert_eq!(d.variants[1].magic.as_ref().unwrap().byte_len(), 3);
        assert_eq!(d.magic_width(), MagicWidth::Mixed);
    }

    #[test]
    fn tag_variant_requires_a_selector() {
        let e = enum_of(r#"enum E { #[bin(tag = 1)] A(u8) }"#);
        assert!(EnumDispatch::parse(&e, None, None).is_err());
        let d = EnumDispatch::parse(&e, Some(sel("kind")), None).unwrap();
        assert_eq!(d.variants[0].role(), VariantRole::TagOnly);
    }

    #[test]
    fn tag_and_magic_compose_on_one_variant() {
        let e = enum_of(r#"enum E { #[bin(tag = 1, magic = b"LI")] A(u32) }"#);
        let d = EnumDispatch::parse(&e, Some(sel("kind")), None).unwrap();
        assert_eq!(d.variants[0].role(), VariantRole::TagAndMagic);
    }

    #[test]
    fn fallback_and_catch_all_roles() {
        let e = enum_of(r#"enum E { #[bin(magic = b"X")] A, Plain(u32), #[catch_all] Other(u8) }"#);
        let d = EnumDispatch::parse(&e, None, None).unwrap();
        assert_eq!(d.variants[1].role(), VariantRole::Fallback);
        assert_eq!(d.variants[2].role(), VariantRole::CatchAll);
    }

    #[test]
    fn rejects_invalid_magic_values() {
        let bad = [
            r#"enum E { #[bin(magic = SOME_CONST)] A }"#, // non-literal
            r#"enum E { #[bin(magic = 1)] A }"#,          // unsuffixed int (ambiguous width)
            r#"enum E { #[bin(magic = 1usize)] A }"#,     // non-byte-aligned/platform width
            r#"enum E { #[bin(magic = u4::new(1))] A }"#, // sub-byte, and a non-literal
        ];
        for src in bad {
            assert!(
                EnumDispatch::parse(&enum_of(src), None, None).is_err(),
                "should reject: {src}"
            );
        }
    }

    #[test]
    fn rejects_two_catch_alls_and_two_fallbacks() {
        let two_catch = r#"enum E { #[catch_all] A(u8), #[catch_all] B(u8) }"#;
        assert!(EnumDispatch::parse(&enum_of(two_catch), None, None).is_err());
        let two_fallback = r#"enum E { A(u8), B(u8) }"#;
        assert!(EnumDispatch::parse(&enum_of(two_fallback), None, None).is_err());
    }

    #[test]
    fn prefix_is_recorded() {
        let e = enum_of(r#"enum E { #[bin(magic = b"X")] A }"#);
        let d = EnumDispatch::parse(&e, None, Some(Magic::Bytes(b"PRE".to_vec()))).unwrap();
        assert_eq!(d.prefix.as_ref().unwrap().byte_len(), 3);
    }
}
