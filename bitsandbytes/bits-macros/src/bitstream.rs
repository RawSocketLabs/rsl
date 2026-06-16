//! Expansion of `#[derive(BitDecode)]` / `#[derive(BitEncode)]` — the bit-stream
//! message codec (spike).
//!
//! Each generates an impl that reads/writes the struct's named fields **in
//! declaration order** from a `bits::BitReader`/`BitWriter` bit cursor. A field
//! is read with `r.read()` / written with `w.write(self.field)`, which works for
//! any `bits::Bits` type (`u1`..`u127`, `#[bitfield]`, `#[derive(BitEnum)]`), so
//! the bit-stream codec composes with the rest of the crate's macros. Nested
//! `#[nested]` messages, `[u8; N]` payloads, `magic`, and `#[br(count = …)]`
//! `Vec`s are supported; the rest of the `#[br]`/`#[bw]` surface is in progress.
//!
//! ## Right-tool guard
//!
//! The bit-stream codec earns its keep only when fields land at non-byte offsets.
//! If a struct's fields are **all byte-aligned** (every width a multiple of 8),
//! the cursor never leaves byte boundaries, so `#[binrw]`/`#[wire]` is the better
//! tool (richer: `magic`/`count`/`args`/`Vec`/nesting). The derives emit a
//! const-eval guard that rejects such a struct, steering the author to the right
//! macro. The escape hatch is `#[bit_stream(allow_byte_aligned)]`.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse::Parser;
use syn::{
    Data, DeriveInput, Fields, FieldsNamed, Ident, ItemStruct, parse_macro_input, parse_quote,
};

/// The const-eval guard's message — names both better tools.
const BYTE_ALIGNED_MSG: &str = "this struct's fields are all byte-aligned, so the \
bit-stream codec (BitDecode/BitEncode) adds nothing over a byte codec: use `#[binrw]` or \
`#[wire]` instead (they also give magic/count/args/Vec/nested messages). Collapse adjacent \
sub-byte fields into a `#[bitfield]`/`#[wire] group(...)`. BitDecode/BitEncode are for fields \
that straddle byte boundaries in the stream (e.g. a 108-bit payload). To keep the bit-stream \
codec for an all-byte-aligned struct anyway, add `#[bit_stream(allow_byte_aligned)]`.";

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
    /// `magic = <expr>` — a leading constant verified on read, emitted on write.
    /// Any `Bits` value, so it can be sub-byte (`u3::new(0b110)`) unlike binrw.
    magic: Option<syn::Expr>,
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
                } else if meta.path.is_ident("magic") {
                    attrs.magic = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error(
                        "unknown `#[bit_stream(...)]` option; expected `allow_byte_aligned`, `bit_order = msb|lsb`, or `magic = <expr>`",
                    ))
                }
            })?;
        }
    }
    Ok(attrs)
}

/// The runtime `BitOrder` token for the struct's declared order.
fn order_token(attrs: &BitStreamAttrs) -> TokenStream2 {
    if attrs.lsb {
        quote!(::bits::__private::BitOrder::Lsb)
    } else {
        quote!(::bits::__private::BitOrder::Msb)
    }
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
    if let syn::Type::Path(p) = &f.ty {
        let seg = p.path.segments.last()?;
        if seg.ident == "Vec" {
            if let syn::PathArguments::AngleBracketed(a) = &seg.arguments {
                if let Some(syn::GenericArgument::Type(t)) = a.args.first() {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// Parses `#[br(count = <expr>)]` from a field, if present. `count` is the only
/// `#[br(...)]` directive so far (more arrive in later Phase 2 chunks). The `expr`
/// may name an earlier field (read into a local before this one).
fn field_count(f: &syn::Field) -> syn::Result<Option<syn::Expr>> {
    let mut count = None;
    for attr in &f.attrs {
        if attr.path().is_ident("br") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("count") {
                    count = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error(
                        "unknown `#[br(...)]` directive; only `count = <expr>` is supported so far",
                    ))
                }
            })?;
        }
    }
    Ok(count)
}

/// The bit-width expression for a field, used by the alignment guard (and, for a
/// fixed message, the `BIT_LEN` sum): a nested message contributes its
/// `FixedBitLen::BIT_LEN`, a fixed `[u8; N]` `N * 8`, a `Bits` leaf its `BITS`, a
/// `Vec<T>` its **element** width (its alignment is the element's). Resolved by
/// the compiler (the macro never computes widths).
fn field_width(f: &syn::Field) -> TokenStream2 {
    let ty = &f.ty;
    if let Some(elem) = vec_elem(f) {
        if is_nested(f) {
            quote!(<#elem as ::bits::__private::FixedBitLen>::BIT_LEN)
        } else {
            quote!(<#elem as ::bits::__private::Bits>::BITS)
        }
    } else if is_nested(f) {
        quote!(<#ty as ::bits::__private::FixedBitLen>::BIT_LEN)
    } else if let Some(len) = byte_array_len(f) {
        quote!(((#len) as u32 * 8))
    } else {
        quote!(<#ty as ::bits::__private::Bits>::BITS)
    }
}

/// The decode statement for one field — a `let #id = …;` binding (so a later
/// `count` can name an earlier field). Variable `Vec<T>` fields loop `count`
/// times; element reads dispatch nested-message vs `Bits` leaf via `#[nested]`.
fn field_read_stmt(f: &syn::Field) -> syn::Result<TokenStream2> {
    let id = f.ident.as_ref().expect("named field");
    let ty = &f.ty;
    let count = field_count(f)?;
    if let Some(elem) = vec_elem(f) {
        let count = count.ok_or_else(|| {
            syn::Error::new_spanned(f, "a `Vec<_>` field needs `#[br(count = <expr>)]`")
        })?;
        // Read one element into `__e`, pinning its type so inference can't drift.
        let read_elem = if is_nested(f) {
            quote! {
                let __e = <#elem as ::bits::__private::BitDecode>::bit_decode(r)
                    .map_err(|e| e.in_field(::core::stringify!(#id)))?;
            }
        } else {
            quote! {
                let __e: #elem = ::bits::__private::Source::read(r)
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
        if count.is_some() {
            return Err(syn::Error::new_spanned(
                f,
                "`#[br(count = …)]` applies only to a `Vec<_>` field",
            ));
        }
        if is_nested(f) {
            Ok(
                quote!(let #id = <#ty as ::bits::__private::BitDecode>::bit_decode(r)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;),
            )
        } else if byte_array_len(f).is_some() {
            Ok(quote!(let #id = ::bits::__private::read_byte_array(r)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;))
        } else {
            Ok(quote!(let #id = r.read().map_err(|e| e.in_field(::core::stringify!(#id)))?;))
        }
    }
}

/// The encode statement for one field — `Vec<T>` writes every element; the count
/// is implied by `len()` (a separate length field is the user's, often `calc`'d).
fn field_write_stmt(f: &syn::Field) -> TokenStream2 {
    let id = f.ident.as_ref().expect("named field");
    let ty = &f.ty;
    if let Some(elem) = vec_elem(f) {
        let write_elem = if is_nested(f) {
            quote!(<#elem as ::bits::__private::BitEncode>::bit_encode(__e, w)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else {
            quote!(::bits::__private::Sink::write(w, *__e)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        };
        quote! {
            for __e in &self.#id {
                #write_elem
            }
        }
    } else if is_nested(f) {
        quote!(<#ty as ::bits::__private::BitEncode>::bit_encode(&self.#id, w)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
    } else if byte_array_len(f).is_some() {
        quote!(::bits::__private::write_byte_array(&self.#id, w)
            .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
    } else {
        quote!(w.write(self.#id).map_err(|e| e.in_field(::core::stringify!(#id)))?;)
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
        terms.push(quote!((::bits::__private::bits_of(&#m) % 8 == 0)));
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
    let name = &input.ident;
    let fields = named_struct(input)?;
    let attrs = parse_bit_stream(input)?;
    let guard = alignment_guard(fields, attrs.allow_byte_aligned, attrs.magic.as_ref());
    let order = order_token(&attrs);
    // `magic`: a leading constant read and verified before the fields. Its width
    // (inferred from the value's type) joins `BIT_LEN`.
    let (magic_read, magic_bits) = match &attrs.magic {
        Some(m) => (
            quote! {
                ::bits::__private::verify_magic(r, #m).map_err(|e| e.in_field("magic"))?;
            },
            quote!(::bits::__private::bits_of(&#m) +),
        ),
        None => (quote!(), quote!()),
    };

    // Read each field into a same-named local (declaration order), so a later
    // `count` directive can reference an earlier field; then build `Self`.
    let ids: Vec<&Ident> = fields
        .named
        .iter()
        .map(|f| f.ident.as_ref().expect("named field"))
        .collect();
    let read_stmts = fields
        .named
        .iter()
        .map(field_read_stmt)
        .collect::<syn::Result<Vec<_>>>()?;

    // A message with a `count`-driven `Vec` is variable-length; only a fixed one
    // also implements `FixedBitLen` (its const length sizes embedded regions).
    let variable = fields.named.iter().any(|f| vec_elem(f).is_some());
    let fixed_bit_len = if variable {
        quote!()
    } else {
        let widths = fields.named.iter().map(field_width);
        quote! {
            impl ::bits::__private::FixedBitLen for #name {
                const BIT_LEN: u32 = #magic_bits 0 #(+ #widths)*;
            }
        }
    };

    Ok(quote! {
        #guard
        #fixed_bit_len
        impl ::bits::BitDecode for #name {
            fn bit_decode<S: ::bits::__private::Source>(
                r: &mut S,
            ) -> ::core::result::Result<Self, ::bits::__private::BitError> {
                #magic_read
                #(#read_stmts)*
                ::core::result::Result::Ok(Self { #(#ids),* })
            }
        }

        impl #name {
            #[doc = "Decode one message from the front of `buf`, advancing it past the bytes consumed (the tail stays in `buf`; transactional on error)."]
            pub fn decode(buf: &mut &[u8]) -> ::core::result::Result<Self, ::bits::__private::BitError> {
                ::bits::__private::decode_consume(buf, #order)
            }
            #[doc = "Decode one message from `bytes` without consuming the caller's buffer (tail-tolerant)."]
            pub fn peek(bytes: &[u8]) -> ::core::result::Result<Self, ::bits::__private::BitError> {
                ::bits::__private::decode_peek(bytes, #order)
            }
            #[doc = "Decode and require every whole byte consumed (errors with `ErrorKind::TrailingBytes` otherwise)."]
            pub fn decode_exact(bytes: &[u8]) -> ::core::result::Result<Self, ::bits::__private::BitError> {
                ::bits::__private::decode_exact(bytes, #order)
            }
            #[doc = "Decode from an explicit bit source (a `BitReader` cursor or a streaming reader)."]
            pub fn decode_from<S: ::bits::__private::Source>(
                r: &mut S,
            ) -> ::core::result::Result<Self, ::bits::__private::BitError> {
                <Self as ::bits::BitDecode>::bit_decode(r)
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
    let name = &input.ident;
    let fields = named_struct(input)?;
    let attrs = parse_bit_stream(input)?;
    let guard = alignment_guard(fields, attrs.allow_byte_aligned, attrs.magic.as_ref());
    let order = order_token(&attrs);
    // `magic`: emit the leading constant before the fields (matched read/write).
    let magic_write = match &attrs.magic {
        Some(m) => quote! {
            ::bits::__private::Sink::write(w, #m).map_err(|e| e.in_field("magic"))?;
        },
        None => quote!(),
    };
    let writes = fields.named.iter().map(field_write_stmt);
    Ok(quote! {
        #guard
        impl ::bits::BitEncode for #name {
            fn bit_encode<K: ::bits::__private::Sink>(
                &self,
                w: &mut K,
            ) -> ::core::result::Result<(), ::bits::__private::BitError> {
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
            ) -> ::core::result::Result<(), ::bits::__private::BitError> {
                ::bits::__private::encode_to_writer(self, w, #order)
            }
            #[doc = "Encode to a `Vec<u8>`."]
            pub fn to_bytes(&self) -> ::core::result::Result<::std::vec::Vec<u8>, ::bits::__private::BitError> {
                ::bits::__private::encode_to_vec(self, #order)
            }
            #[doc = "Encode into an explicit bit sink (a `BitWriter`)."]
            pub fn encode_into<K: ::bits::__private::Sink>(
                &self,
                w: &mut K,
            ) -> ::core::result::Result<(), ::bits::__private::BitError> {
                <Self as ::bits::BitEncode>::bit_encode(self, w)
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
// duplication front-end (the same shape as `#[wire]` lowering to `#[binrw]`).
// Field directives (`#[br]`/`#[bw]`/`#[brw]`) are added in later chunks and ride
// through as derive helper attributes. (Trait rename BitDecode->Decode: Phase 5.)
// ---------------------------------------------------------------------------

/// Parsed struct-level `#[bin(...)]` options.
#[derive(Default)]
struct BinArgs {
    read_only: bool,
    write_only: bool,
    no_builder: bool,
    lsb: bool,
    allow_byte_aligned: bool,
    magic: Option<syn::Expr>,
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
        } else if meta.path.is_ident("allow_byte_aligned") {
            args.allow_byte_aligned = true;
        } else if meta.path.is_ident("bit_order") {
            let v: Ident = meta.value()?.parse()?;
            match v.to_string().as_str() {
                "msb" => args.lsb = false,
                "lsb" => args.lsb = true,
                _ => return Err(meta.error("expected `msb` or `lsb`")),
            }
        } else if meta.path.is_ident("magic") {
            args.magic = Some(meta.value()?.parse()?);
        } else {
            return Err(meta.error(
                "unknown `#[bin(...)]` option; expected one of: read_only, write_only, \
                 no_builder, bit_order = msb|lsb, magic = <expr>, allow_byte_aligned",
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

    // Lower to the codec/builder derives. read_only ⇒ Decode only (no builder);
    // write_only ⇒ Encode + builder; default ⇒ both + builder.
    let mut derives: Vec<TokenStream2> = Vec::new();
    if !args.write_only {
        derives.push(quote!(::bits::BitDecode));
    }
    if !args.read_only {
        derives.push(quote!(::bits::BitEncode));
        if !args.no_builder {
            derives.push(quote!(::bits::BitsBuilder));
        }
    }

    // Lower the struct-level options to a `#[bit_stream(...)]` helper attribute.
    let mut bs: Vec<TokenStream2> = Vec::new();
    if args.lsb {
        bs.push(quote!(bit_order = lsb));
    }
    if args.allow_byte_aligned {
        bs.push(quote!(allow_byte_aligned));
    }
    if let Some(m) = &args.magic {
        bs.push(quote!(magic = #m));
    }
    let bit_stream = if bs.is_empty() {
        quote!()
    } else {
        quote!(#[bit_stream(#(#bs),*)])
    };

    Ok(quote! {
        #[derive(#(#derives),*)]
        #bit_stream
        #s
    })
}

// ---------------------------------------------------------------------------
// `#[bitwire]` — the dispatch macro (spike, DESIGN §11 DD1).
//
// Lowers to a `#[binrw]` struct: byte-aligned fields keep their `#[br]/#[bw]/
// #[brw]` attributes (binrw runs them — magic/count/args/…); a field marked
// `#[bits]` is a `BitDecode`/`BitEncode` *region* wired into binrw via its own
// `parse_with`/`write_with` escape hatch. One vocabulary, two backends.
// ---------------------------------------------------------------------------

/// Entry for `#[cfg(feature = "binrw")]` `#[bitwire(big|little)]`.
#[cfg(feature = "binrw")]
pub fn expand_bitwire(attr: TokenStream, item: TokenStream) -> TokenStream {
    match bitwire_inner(attr.into(), item.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[cfg(feature = "binrw")]
fn bitwire_inner(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let endian = parse_endian(attr)?;
    let mut s: ItemStruct = syn::parse2(item)?;
    let fields = match &mut s.fields {
        Fields::Named(n) => n,
        _ => {
            return Err(syn::Error::new_spanned(
                &s.ident,
                "#[bitwire] requires a struct with named fields",
            ));
        }
    };

    // For each `#[bits]` field: strip the marker, inject the binrw bridge
    // attributes, and assert the region is byte-aligned (so it occupies whole
    // bytes in the stream).
    let mut asserts = Vec::new();
    for field in &mut fields.named {
        let mut is_bits = false;
        field.attrs.retain(|a| {
            if a.path().is_ident("bits") {
                is_bits = true;
                false
            } else {
                true
            }
        });
        if is_bits {
            let ty = &field.ty;
            asserts.push(quote! {
                const _: () = assert!(
                    <#ty as ::bits::__private::FixedBitLen>::BIT_LEN % 8 == 0,
                    "#[bits] region must be byte-aligned (its fields' widths must sum to a multiple of 8) to embed in the byte stream"
                );
            });
            field.attrs.push(parse_quote!(
                #[br(parse_with = ::bits::__private::read_bits_region)]
            ));
            field.attrs.push(parse_quote!(
                #[bw(write_with = ::bits::__private::write_bits_region)]
            ));
        }
    }

    Ok(quote! {
        #(#asserts)*
        #[::binrw::binrw]
        #[brw(#endian)]
        #s
    })
}

/// Parses the `big`/`little` endian argument (default `big`).
#[cfg(feature = "binrw")]
fn parse_endian(attr: TokenStream2) -> syn::Result<Ident> {
    if attr.is_empty() {
        return Ok(Ident::new("big", Span::call_site()));
    }
    let id: Ident = syn::parse2(attr)?;
    match id.to_string().as_str() {
        "big" | "little" => Ok(id),
        _ => Err(syn::Error::new_spanned(&id, "expected `big` or `little`")),
    }
}
