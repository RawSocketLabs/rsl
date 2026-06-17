//! Expansion of `#[derive(BitDecode)]` / `#[derive(BitEncode)]` — the bit-stream
//! message codec (spike).
//!
//! Each generates an impl that reads/writes the struct's named fields **in
//! declaration order** from a `bnb::BitReader`/`BitWriter` bit cursor. A field
//! is read with `r.read()` / written with `w.write(self.field)`, which works for
//! any `bnb::Bits` type (`u1`..`u127`, `#[bitfield]`, `#[derive(BitEnum)]`), so
//! the bit-stream codec composes with the rest of the crate's macros. Nested
//! `#[nested]` messages, `[u8; N]` payloads, `magic`, `#[br(count = …)]` `Vec`s,
//! `ctx` parameterization, `#[br(temp)]`/`#[bw(calc = …)]`, `#[br(if(…))]`
//! conditional `Option`s, `#[br(map/try_map = …)]`/`#[bw(map = …)]` transforms, and
//! `#[reserved]`/`#[reserved_with(…)]` bits are supported (`temp`/`calc`/`reserved`
//! via `#[bin]`, which generates the codec directly); the rest of the
//! `#[br]`/`#[bw]` surface is in progress.
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
    /// Any `Bits` value, so it can be sub-byte (`u3::new(0b110)`) unlike binrw.
    magic: Option<syn::Expr>,
    /// `ctx(name: Ty, …)` — context this type needs from its parent (binrw
    /// `import`). When present the type gets `decode_with`/`encode_with` (it does
    /// **not** implement `BitDecode`/`BitEncode`, which take no context).
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
    let bit = if attrs.lsb {
        quote!(::bnb::__private::BitOrder::Lsb)
    } else {
        quote!(::bnb::__private::BitOrder::Msb)
    };
    let byte = if attrs.little {
        quote!(::bnb::__private::ByteOrder::Little)
    } else {
        quote!(::bnb::__private::ByteOrder::Big)
    };
    quote!(::bnb::__private::Layout { bit: #bit, byte: #byte })
}

/// Whether a field is a **nested message** (marked `#[nested]`) — a
/// `BitDecode`/`BitEncode` struct recursed into — rather than a `Bits` leaf.
/// (Phase 1 marker; the end-state can auto-detect via universal `Bits` impls.)
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
    /// `#[br(ignore)]` — an in-memory-only field: `Default::default()` on read (no
    /// input consumed), skipped on write. Zero wire bits.
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
    /// later fields re-read from the same offset; skipped on write. Needs a
    /// [`SeekSource`](bnb::SeekSource) (a slice cursor) — errors on a forward stream.
    restore_position: bool,
}

/// One `#[br(...)]` directive. A hand-rolled parser (not `parse_nested_meta`)
/// because `if` is a keyword and can't be read as a meta path ident.
enum BrDirective {
    Count(syn::Expr),
    Ctx(Vec<Ident>),
    Temp,
    Ignore,
    If(syn::Expr),
    Map(syn::Expr),
    TryMap(syn::Expr),
    ParseWith(syn::Expr),
    PadBefore(syn::Expr),
    PadAfter(syn::Expr),
    AlignBefore,
    AlignAfter,
    RestorePosition,
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
                "ignore" => Ok(BrDirective::Ignore),
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
                _ => Err(syn::Error::new_spanned(
                    kw,
                    "unknown `#[br(...)]` directive; expected `count`, `ctx`, `temp`, `ignore`, `if`, `map`, `try_map`, `parse_with`, `pad_before/after`, `align_before/after`, or `restore_position`",
                )),
            }
        }
    }
}

/// Parses a field's `#[br(count = …, ctx { … }, temp, if(…))]` and `#[bw(calc = …)]`.
fn parse_field_br(f: &syn::Field) -> syn::Result<FieldBr> {
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
                    BrDirective::Ignore => br.ignore = true,
                    BrDirective::If(e) => br.cond = Some(e),
                    BrDirective::Map(e) => br.map = Some(e),
                    BrDirective::TryMap(e) => br.try_map = Some(e),
                    BrDirective::ParseWith(e) => br.parse_with = Some(e),
                    BrDirective::PadBefore(e) => br.pad_before = Some(e),
                    BrDirective::PadAfter(e) => br.pad_after = Some(e),
                    BrDirective::AlignBefore => br.align_before = true,
                    BrDirective::AlignAfter => br.align_after = true,
                    BrDirective::RestorePosition => br.restore_position = true,
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

/// Whether a field threads context to a `ctx` child (`#[br(ctx { … })]`). Such a
/// field's width is indeterminate (the child isn't `Bits`/`FixedBitLen`), so it
/// makes the struct variable-length and exempt from the alignment guard. Parse
/// errors are deferred to [`field_read_stmt`], which validates properly.
fn field_has_ctx(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| br.ctx.is_some())
}

/// Whether a field is `#[br(temp)]` (read into a local, not stored).
fn field_is_temp(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| br.temp)
}

/// Whether a field is `#[br(ignore)]` (in-memory only — defaulted on read, not
/// written, zero wire bits).
fn field_is_ignore(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| br.ignore)
}

/// Whether a field carries a positioning directive (`pad_*`/`align_*`). Those shift
/// the cursor, so the struct's fixed length / alignment can't be computed statically.
fn field_has_positioning(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| {
        br.pad_before.is_some()
            || br.pad_after.is_some()
            || br.align_before
            || br.align_after
            || br.restore_position
    })
}

/// Whether a field is conditional (`#[br(if(...))]`). Like a `ctx` child, its
/// width is indeterminate (present or absent), so it makes the struct
/// variable-length and exempt from the alignment guard.
fn field_is_conditional(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| br.cond.is_some())
}

/// Whether a field has a custom codec (`map`/`try_map`/`parse_with`/`write_with`).
/// Its type isn't necessarily `Bits` (the wire shape lives in the converter/fn), so
/// its width is indeterminate.
fn field_is_mapped(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| {
        br.map.is_some()
            || br.try_map.is_some()
            || br.bw_map.is_some()
            || br.parse_with.is_some()
            || br.write_with.is_some()
    })
}

/// A reserved field — on the wire (its type gives the width) but not stored: read
/// and discarded (lenient), written as a constant.
enum Reserved {
    /// `#[reserved]` — written as the type's zero value.
    Zero,
    /// `#[reserved_with(<expr>)]` — written as `<expr>` (e.g. a must-be-one pattern).
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

/// Whether a field is reserved (`#[reserved]`/`#[reserved_with]`). Reserved fields
/// have a fixed width (their type's), so — unlike `temp` — they still count toward
/// `BIT_LEN` and the guard; they are just not stored.
fn field_is_reserved(f: &syn::Field) -> bool {
    f.attrs
        .iter()
        .any(|a| a.path().is_ident("reserved") || a.path().is_ident("reserved_with"))
}

/// Whether a field attribute is one `#[bin]` consumes itself (`#[nested]`/`#[br]`/
/// `#[bw]` for the codec, `#[builder]` for the builder) and must strip from the
/// struct it emits — it generates the codec and builder directly, so nothing
/// registers these as helper attributes.
fn is_codec_field_attr(a: &syn::Attribute) -> bool {
    ["nested", "br", "bw", "builder"]
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
    let ty = &f.ty;
    if field_is_ignore(f) {
        return quote!(0u32); // in-memory only: zero wire bits
    }
    if let Some(elem) = vec_elem(f) {
        if is_nested(f) {
            quote!(<#elem as ::bnb::__private::FixedBitLen>::BIT_LEN)
        } else {
            quote!(<#elem as ::bnb::__private::Bits>::BITS)
        }
    } else if is_nested(f) {
        quote!(<#ty as ::bnb::__private::FixedBitLen>::BIT_LEN)
    } else if let Some(len) = byte_array_len(f) {
        quote!(((#len) as u32 * 8))
    } else {
        quote!(<#ty as ::bnb::__private::Bits>::BITS)
    }
}

/// Positioning statements emitted before/after a field: `align_*` skips to the next
/// byte boundary, `pad_*` skips a bit count.
fn pad_read_tokens(align: bool, pad: Option<&syn::Expr>) -> TokenStream2 {
    let align = align.then(|| quote!(::bnb::__private::align_read(r)?;));
    let pad = pad.map(|n| quote!(::bnb::__private::skip_read(r, #n)?;));
    quote!(#align #pad)
}

fn pad_write_tokens(align: bool, pad: Option<&syn::Expr>) -> TokenStream2 {
    let align = align.then(|| quote!(::bnb::__private::align_write(w)?;));
    let pad = pad.map(|n| quote!(::bnb::__private::skip_write(w, #n)?;));
    quote!(#align #pad)
}

/// The decode statement for one field — a `let #id = …;` binding, wrapped with any
/// `pad_*`/`align_*` positioning. A later `count` can name an earlier field.
fn field_read_stmt(f: &syn::Field) -> syn::Result<TokenStream2> {
    let br = parse_field_br(f)?;
    let pre = pad_read_tokens(br.align_before, br.pad_before.as_ref());
    let post = pad_read_tokens(br.align_after, br.pad_after.as_ref());
    let mut core = field_read_core(f)?;
    if br.restore_position {
        // Peek: save the offset, read the field, rewind so later fields re-read it.
        core = quote! {
            let __pos = ::bnb::__private::Source::bit_pos(r);
            #core
            ::bnb::__private::Source::seek_to_bit(r, __pos)?;
        };
    }
    Ok(quote!(#pre #core #post))
}

/// The core decode statement (without positioning).
fn field_read_core(f: &syn::Field) -> syn::Result<TokenStream2> {
    let id = f.ident.as_ref().expect("named field");
    let ty = &f.ty;
    let br = parse_field_br(f)?;
    // `#[reserved]`: consume the bits but discard the value (lenient — a non-zero
    // reserved value is not rejected; use `magic` to enforce one).
    if field_is_reserved(f) {
        return Ok(
            quote!(let _: #ty = r.read().map_err(|e| e.in_field(::core::stringify!(#id)))?;),
        );
    }
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
            quote!(<#inner as ::bnb::__private::BitDecode>::bit_decode(r)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?)
        } else {
            quote! {{
                let __v: #inner = ::bnb::__private::Source::read(r)
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
        return Ok(quote!(let #id: #ty = ::bnb::__private::read_mapped(r, #map)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;));
    }
    if let Some(try_map) = &br.try_map {
        return Ok(
            quote!(let #id: #ty = ::bnb::__private::read_try_mapped(r, #try_map)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;),
        );
    }
    // `parse_with`: the escape hatch — a custom `f(r) -> Result<T, BitError>`.
    if let Some(f) = &br.parse_with {
        return Ok(quote!(let #id: #ty = (#f)(r)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;));
    }
    if let Some(elem) = vec_elem(f) {
        let count = br.count.ok_or_else(|| {
            syn::Error::new_spanned(f, "a `Vec<_>` field needs `#[br(count = <expr>)]`")
        })?;
        // Read one element into `__e`, pinning its type so inference can't drift.
        let read_elem = if let Some(names) = &br.ctx {
            let lit = ctx_literal(&ctx_struct_ty(elem)?, names, None);
            quote! {
                let __e = <#elem>::decode_with(r, #lit)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            }
        } else if is_nested(f) {
            quote! {
                let __e = <#elem as ::bnb::__private::BitDecode>::bit_decode(r)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            }
        } else {
            quote! {
                let __e: #elem = ::bnb::__private::Source::read(r)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            }
        };
        // No untrusted pre-allocation: `count` is attacker-controlled, so grow the
        // Vec by pushing (bounded by the input — each element consumes ≥1 bit).
        Ok(quote! {
            let #id = {
                let __n = (#count) as usize;
                let mut __v: ::std::vec::Vec<#elem> = ::std::vec::Vec::new();
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
            Ok(quote!(let #id = <#ty>::decode_with(r, #lit)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;))
        } else if is_nested(f) {
            Ok(
                quote!(let #id = <#ty as ::bnb::__private::BitDecode>::bit_decode(r)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;),
            )
        } else if byte_array_len(f).is_some() {
            Ok(quote!(let #id = ::bnb::__private::read_byte_array(r)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;))
        } else {
            // Pin the leaf type explicitly: a `temp` field is not stored in `Self`,
            // so the construction can't infer it.
            Ok(quote!(let #id: #ty = r.read().map_err(|e| e.in_field(::core::stringify!(#id)))?;))
        }
    }
}

/// The encode statement for one field, wrapped with any `pad_*`/`align_*`. `Vec<T>`
/// writes every element; the count is implied by `len()` (a separate length field
/// is the user's, often `calc`'d). `field_set` is the parent's field names, for
/// resolving a `ctx { … }` pass.
fn field_write_stmt(f: &syn::Field, field_set: &[&Ident]) -> syn::Result<TokenStream2> {
    let br = parse_field_br(f)?;
    let pre = pad_write_tokens(br.align_before, br.pad_before.as_ref());
    let post = pad_write_tokens(br.align_after, br.pad_after.as_ref());
    // A `restore_position` field is a read-side peek (it overlaps later data), so it
    // is not written — the overlapping field emits those bytes.
    let core = if br.restore_position {
        quote!()
    } else {
        field_write_core(f, field_set)?
    };
    Ok(quote!(#pre #core #post))
}

/// The core encode statement (without positioning).
fn field_write_core(f: &syn::Field, field_set: &[&Ident]) -> syn::Result<TokenStream2> {
    let id = f.ident.as_ref().expect("named field");
    let ty = &f.ty;
    // `#[reserved]`: write the type's zero (or the `reserved_with` value), pinned to
    // the field's type so the width is unambiguous.
    if let Some(reserved) = field_reserved(f)? {
        let value = match reserved {
            Reserved::Zero => quote!(<#ty as ::bnb::__private::Bits>::from_bits(0)),
            Reserved::With(expr) => {
                let expr = *expr;
                quote!({ let __r: #ty = #expr; __r })
            }
        };
        return Ok(quote!(::bnb::__private::Sink::write(w, #value)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;));
    }
    let br = parse_field_br(f)?;
    // `ignore`: in-memory only — emit nothing.
    if br.ignore {
        return Ok(quote!());
    }
    // `calc`: write a value computed from the other fields rather than `self.#id`,
    // pinned to the field's declared type so the wire width is unambiguous.
    if let Some(calc) = &br.calc {
        return Ok(quote! {
            {
                let __calc: #ty = #calc;
                ::bnb::__private::Sink::write(w, __calc)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            }
        });
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
        return Ok(quote!(::bnb::__private::write_mapped(w, &self.#id, #bw_map)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;));
    }
    // `write_with`: the escape hatch — a custom `f(&self.field, w) -> Result<()>`.
    if let Some(f) = &br.write_with {
        return Ok(quote!((#f)(&self.#id, w)
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
            quote!(<#inner as ::bnb::__private::BitEncode>::bit_encode(__v, w)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else {
            quote!(::bnb::__private::Sink::write(w, *__v)
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
            let lit = ctx_literal(&ctx_struct_ty(elem)?, names, Some(field_set));
            quote!(<#elem>::encode_with(__e, w, #lit)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else if is_nested(f) {
            quote!(<#elem as ::bnb::__private::BitEncode>::bit_encode(__e, w)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else {
            quote!(::bnb::__private::Sink::write(w, *__e)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        };
        Ok(quote! {
            for __e in &self.#id {
                #write_elem
            }
        })
    } else if let Some(names) = &br.ctx {
        let lit = ctx_literal(&ctx_struct_ty(ty)?, names, Some(field_set));
        Ok(quote!(<#ty>::encode_with(&self.#id, w, #lit)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;))
    } else if is_nested(f) {
        Ok(
            quote!(<#ty as ::bnb::__private::BitEncode>::bit_encode(&self.#id, w)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;),
        )
    } else if byte_array_len(f).is_some() {
        Ok(quote!(::bnb::__private::write_byte_array(&self.#id, w)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;))
    } else {
        Ok(quote!(w.write(self.#id).map_err(|e| e.in_field(::core::stringify!(#id)))?;))
    }
}

/// A const-eval assertion that the struct is *not* entirely byte-aligned (the
/// bit-stream codec would otherwise be the wrong tool). Empty/opted-out → no guard.
/// A sub-byte `magic` counts as a non-byte-aligned element (binrw can't express
/// one), so it suppresses the guard just like a sub-byte field.
fn alignment_guard(fields: &FieldsNamed, allow: bool, magic: Option<&syn::Expr>) -> TokenStream2 {
    if allow || (fields.named.is_empty() && magic.is_none()) {
        return quote!();
    }
    let mut terms: Vec<TokenStream2> = fields
        .named
        .iter()
        .map(|f| {
            let w = field_width(f);
            quote!((#w % 8 == 0))
        })
        .collect();
    if let Some(m) = magic {
        terms.push(quote!((::bnb::__private::bits_of(&#m) % 8 == 0)));
    }
    quote! {
        const _: () = {
            assert!(!(true #(&& #terms)*), #BYTE_ALIGNED_MSG);
        };
    }
}

pub fn expand_decode(item: TokenStream) -> TokenStream {
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
) -> syn::Result<TokenStream2> {
    // `ctx`/`if` anywhere makes widths/alignment indeterminable: exempt from the
    // guard and never `FixedBitLen`.
    let indeterminate = !attrs.ctx.is_empty()
        || fields.named.iter().any(|f| {
            field_has_ctx(f)
                || field_is_conditional(f)
                || field_is_mapped(f)
                || field_has_positioning(f)
        });
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
                ::bnb::__private::verify_magic(r, #m).map_err(|e| e.in_field("magic"))?;
            },
            quote!(::bnb::__private::bits_of(&#m) +),
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
        .filter(|f| !field_is_temp(f) && !field_is_reserved(f))
        .map(|f| f.ident.as_ref().expect("named field"))
        .collect();
    let read_stmts = fields
        .named
        .iter()
        .map(field_read_stmt)
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
                pub fn decode_with<S: ::bnb::__private::Source>(
                    r: &mut S,
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<Self, ::bnb::__private::BitError> {
                    #(#ctx_binds)*
                    #magic_read
                    #(#read_stmts)*
                    ::core::result::Result::Ok(Self { #(#ids),* })
                }
                #[doc = "Decode from bytes with context, requiring every whole byte consumed."]
                pub fn decode_with_exact(
                    bytes: &[u8],
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<Self, ::bnb::__private::BitError> {
                    ::bnb::__private::decode_exact_with(bytes, #layout, |r| Self::decode_with(r, __ctx))
                }
            }
            // ctx Layer 2: the polymorphic companion, so generic combinators can take
            // this type via `T: DecodeWith<#ctx_name>`.
            impl ::bnb::DecodeWith<#ctx_name> for #name {
                fn decode_with<S: ::bnb::__private::Source>(
                    r: &mut S,
                    args: #ctx_name,
                ) -> ::core::result::Result<Self, ::bnb::__private::BitError> {
                    <#name>::decode_with(r, args)
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
            impl ::bnb::__private::FixedBitLen for #name {
                const BIT_LEN: u32 = #magic_bits 0 #(+ #widths)*;
            }
        }
    };

    Ok(quote! {
        #guard
        #fixed_bit_len
        impl ::bnb::BitDecode for #name {
            fn bit_decode<S: ::bnb::__private::Source>(
                r: &mut S,
            ) -> ::core::result::Result<Self, ::bnb::__private::BitError> {
                #magic_read
                #(#read_stmts)*
                ::core::result::Result::Ok(Self { #(#ids),* })
            }
        }

        impl #name {
            #[doc = "Decode one message from the front of `buf`, advancing it past the bytes consumed (the tail stays in `buf`; transactional on error)."]
            pub fn decode(buf: &mut &[u8]) -> ::core::result::Result<Self, ::bnb::__private::BitError> {
                ::bnb::__private::decode_consume(buf, #layout)
            }
            #[doc = "Decode one message from `bytes` without consuming the caller's buffer (tail-tolerant)."]
            pub fn peek(bytes: &[u8]) -> ::core::result::Result<Self, ::bnb::__private::BitError> {
                ::bnb::__private::decode_peek(bytes, #layout)
            }
            #[doc = "Decode and require every whole byte consumed (errors with `ErrorKind::TrailingBytes` otherwise)."]
            pub fn decode_exact(bytes: &[u8]) -> ::core::result::Result<Self, ::bnb::__private::BitError> {
                ::bnb::__private::decode_exact(bytes, #layout)
            }
            #[doc = "Decode from an explicit bit source (a `BitReader` cursor or a streaming reader)."]
            pub fn decode_from<S: ::bnb::__private::Source>(
                r: &mut S,
            ) -> ::core::result::Result<Self, ::bnb::__private::BitError> {
                <Self as ::bnb::BitDecode>::bit_decode(r)
            }
        }
    })
}

pub fn expand_encode(item: TokenStream) -> TokenStream {
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
    )
}

/// Generates the encode side (`BitEncode` + entry points, or `encode_with` for a
/// `ctx` type). Shared by `#[derive(BitEncode)]` and `#[bin]`. `calc` fields write
/// a computed value; `temp` fields (no `self` field) are written via their `calc`.
fn gen_encode(
    name: &Ident,
    fields: &FieldsNamed,
    attrs: &BitStreamAttrs,
) -> syn::Result<TokenStream2> {
    let indeterminate = !attrs.ctx.is_empty()
        || fields.named.iter().any(|f| {
            field_has_ctx(f)
                || field_is_conditional(f)
                || field_is_mapped(f)
                || field_has_positioning(f)
        });
    let guard = alignment_guard(
        fields,
        attrs.allow_byte_aligned || indeterminate,
        attrs.magic.as_ref(),
    );
    let layout = layout_token(attrs);
    // `magic`: emit the leading constant before the fields (matched read/write).
    let magic_write = match &attrs.magic {
        Some(m) => quote! {
            ::bnb::__private::Sink::write(w, #m).map_err(|e| e.in_field("magic"))?;
        },
        None => quote!(),
    };
    // Only stored (non-`temp`) fields exist on `self`, for `ctx { … }` resolution.
    let field_set: Vec<&Ident> = fields
        .named
        .iter()
        .filter(|f| !field_is_temp(f) && !field_is_reserved(f))
        .map(|f| f.ident.as_ref().expect("named field"))
        .collect();
    let writes = fields
        .named
        .iter()
        .map(|f| field_write_stmt(f, &field_set))
        .collect::<syn::Result<Vec<_>>>()?;

    // `ctx(...)`: the dual of decode — inherent `encode_with`/`to_bytes_with`
    // (binding the ctx params as locals), not a `BitEncode` impl.
    if !attrs.ctx.is_empty() {
        let ctx_name = ctx_struct_ident(name);
        let ctx_binds = attrs.ctx.iter().map(|(n, _)| quote!(let #n = __ctx.#n;));
        return Ok(quote! {
            #guard
            impl #name {
                #[doc = "Encode to a bit sink, given the context this type declares via `ctx(...)`."]
                #[allow(unused_variables)] // a ctx param may be used on only one side
                pub fn encode_with<K: ::bnb::__private::Sink>(
                    &self,
                    w: &mut K,
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<(), ::bnb::__private::BitError> {
                    #(#ctx_binds)*
                    #magic_write
                    #(#writes)*
                    ::core::result::Result::Ok(())
                }
                #[doc = "Encode to a `Vec<u8>` with context."]
                pub fn to_bytes_with(
                    &self,
                    __ctx: #ctx_name,
                ) -> ::core::result::Result<::std::vec::Vec<u8>, ::bnb::__private::BitError> {
                    ::bnb::__private::encode_to_vec_with(#layout, |w| self.encode_with(w, __ctx))
                }
            }
            // ctx Layer 2: the polymorphic companion (dual of `DecodeWith`).
            impl ::bnb::EncodeWith<#ctx_name> for #name {
                fn encode_with<K: ::bnb::__private::Sink>(
                    &self,
                    w: &mut K,
                    args: #ctx_name,
                ) -> ::core::result::Result<(), ::bnb::__private::BitError> {
                    <#name>::encode_with(self, w, args)
                }
            }
        });
    }

    Ok(quote! {
        #guard
        impl ::bnb::BitEncode for #name {
            fn bit_encode<K: ::bnb::__private::Sink>(
                &self,
                w: &mut K,
            ) -> ::core::result::Result<(), ::bnb::__private::BitError> {
                #magic_write
                #(#writes)*
                ::core::result::Result::Ok(())
            }
        }

        impl #name {
            #[doc = "Encode to any `std::io::Write` (socket, file, `Vec`)."]
            pub fn encode<W: ::std::io::Write>(
                &self,
                w: &mut W,
            ) -> ::core::result::Result<(), ::bnb::__private::BitError> {
                ::bnb::__private::encode_to_writer(self, w, #layout)
            }
            #[doc = "Encode to a `Vec<u8>`."]
            pub fn to_bytes(&self) -> ::core::result::Result<::std::vec::Vec<u8>, ::bnb::__private::BitError> {
                ::bnb::__private::encode_to_vec(self, #layout)
            }
            #[doc = "Encode into an explicit bit sink (a `BitWriter`)."]
            pub fn encode_into<K: ::bnb::__private::Sink>(
                &self,
                w: &mut K,
            ) -> ::core::result::Result<(), ::bnb::__private::BitError> {
                <Self as ::bnb::BitEncode>::bit_encode(self, w)
            }
        }
    })
}

// ---------------------------------------------------------------------------
// `#[bin]` — the unified codec attribute (Phase 2 foundation, ROADMAP).
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
}

/// Entry for `#[bin(...)]`.
pub fn expand_bin(attr: TokenStream, item: TokenStream) -> TokenStream {
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
        } else {
            return Err(meta.error(
                "unknown `#[bin(...)]` option; expected one of: read_only, write_only, \
                 no_builder, forward_only, big, little, bit_order = msb|lsb, magic = <expr>, \
                 ctx(name: Ty, …), validate = <path>",
            ));
        }
        Ok(())
    });
    Parser::parse(parser, attr)?;

    let s: ItemStruct = syn::parse(item)?;
    if args.read_only && args.write_only {
        return Err(syn::Error::new_spanned(
            &s.ident,
            "`read_only` and `write_only` are mutually exclusive",
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
            if parse_field_br(f)
                .map(|br| br.restore_position)
                .unwrap_or(false)
            {
                return Err(syn::Error::new_spanned(
                    f,
                    "`#[br(restore_position)]` needs to seek, but the struct is `#[bin(forward_only)]`",
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
    let decode = if args.write_only {
        quote!()
    } else {
        gen_decode(&s.ident, full_fields, &attrs)?
    };
    let encode = if args.read_only {
        quote!()
    } else {
        gen_encode(&s.ident, full_fields, &attrs)?
    };

    // The emitted struct: drop `#[br(temp)]` fields and strip codec-only field
    // attributes (they are not registered helper attrs here — the codec is
    // generated directly, not via the derives).
    let mut clean = s.clone();
    if let Fields::Named(named) = &mut clean.fields {
        named.named = named
            .named
            .iter()
            .filter(|f| !field_is_temp(f) && !field_is_reserved(f))
            .cloned()
            .map(|mut f| {
                f.attrs.retain(|a| !is_codec_field_attr(a));
                f
            })
            .collect();
    }

    // The builder is generated directly from the stored fields (so it can run the
    // `validate` hook via `builder::generate`'s post_build, and so `temp`/`reserved`
    // fields are absent from it).
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
                    .map_err(|__e| ::bnb::BuilderError::invalid(__e.to_string()))?;
            }
        });
        let mut bfields = Vec::new();
        for f in &full_fields.named {
            if field_is_temp(f) || field_is_reserved(f) {
                continue; // not stored, so not a builder field
            }
            let ident = f.ident.clone().expect("named field");
            let ty = f.ty.clone();
            let mut default = crate::builder::FieldDefault::Required;
            for attr in &f.attrs {
                if let Some(d) = crate::builder::parse_builder_attr(attr)? {
                    default = d;
                }
            }
            bfields.push(crate::builder::BField { ident, ty, default });
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
        quote! {
            #[derive(Clone)]
            #[doc = "Context for decoding/encoding the matching `#[bin(ctx(...))]` type."]
            #vis struct #ctx_name { #(#decls),* }
        }
    };

    Ok(quote! {
        #ctx_struct
        #clean
        #builder
        #decode
        #encode
    })
}
