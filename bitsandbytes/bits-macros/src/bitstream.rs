//! Expansion of `#[derive(BitDecode)]` / `#[derive(BitEncode)]` — the bit-stream
//! message codec (spike).
//!
//! Each generates an impl that reads/writes the struct's named fields **in
//! declaration order** from a `bits::BitReader`/`BitWriter` bit cursor. A field
//! is read with `r.read()` / written with `w.write(self.field)`, which works for
//! any `bits::Bits` type (`u1`..`u127`, `#[bitfield]`, `#[derive(BitEnum)]`), so
//! the bit-stream codec composes with the rest of the crate's macros. Nested
//! `BitDecode` messages and `#[br]`-style attributes are future work.
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
                } else {
                    Err(meta.error(
                        "unknown `#[bit_stream(...)]` option; expected `allow_byte_aligned` or `bit_order = msb|lsb`",
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

/// The bit-width expression for a field: a nested message contributes its
/// `BIT_LEN`, a fixed `[u8; N]` byte array `N * 8`, a `Bits` leaf its `BITS`.
/// Resolved by the compiler (the macro never computes widths).
fn field_width(f: &syn::Field) -> TokenStream2 {
    let ty = &f.ty;
    if is_nested(f) {
        quote!(<#ty as ::bits::__private::BitDecode>::BIT_LEN)
    } else if let Some(len) = byte_array_len(f) {
        quote!(((#len) as u32 * 8))
    } else {
        quote!(<#ty as ::bits::__private::Bits>::BITS)
    }
}

/// A const-eval assertion that the struct is *not* entirely byte-aligned (the
/// bit-stream codec would otherwise be the wrong tool). Empty/opted-out → no guard.
fn alignment_guard(fields: &FieldsNamed, allow: bool) -> TokenStream2 {
    if allow || fields.named.is_empty() {
        return quote!();
    }
    let aligned = fields.named.iter().map(|f| {
        let w = field_width(f);
        quote!((#w % 8 == 0))
    });
    quote! {
        const _: () = {
            assert!(!(true #(&& #aligned)*), #BYTE_ALIGNED_MSG);
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
    let guard = alignment_guard(fields, attrs.allow_byte_aligned);
    let order = order_token(&attrs);
    let widths = fields.named.iter().map(field_width);
    let reads = fields.named.iter().map(|f| {
        let id = f.ident.as_ref().expect("named field");
        let ty = &f.ty;
        // Attach the field name to any error for a position-aware "span".
        if is_nested(f) {
            quote!(#id: <#ty as ::bits::__private::BitDecode>::bit_decode(r)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?)
        } else if byte_array_len(f).is_some() {
            quote!(#id: ::bits::__private::read_byte_array(r)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?)
        } else {
            quote!(#id: r.read().map_err(|e| e.in_field(::core::stringify!(#id)))?)
        }
    });
    Ok(quote! {
        #guard
        impl ::bits::BitDecode for #name {
            const BIT_LEN: u32 = 0 #(+ #widths)*;

            fn bit_decode<S: ::bits::__private::Source>(
                r: &mut S,
            ) -> ::core::result::Result<Self, ::bits::__private::BitError> {
                ::core::result::Result::Ok(Self { #(#reads,)* })
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
    let guard = alignment_guard(fields, attrs.allow_byte_aligned);
    let order = order_token(&attrs);
    let writes = fields.named.iter().map(|f| {
        let id = f.ident.as_ref().expect("named field");
        let ty = &f.ty;
        if is_nested(f) {
            quote!(<#ty as ::bits::__private::BitEncode>::bit_encode(&self.#id, w)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else if byte_array_len(f).is_some() {
            quote!(::bits::__private::write_byte_array(&self.#id, w)
                .map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        } else {
            quote!(w.write(self.#id).map_err(|e| e.in_field(::core::stringify!(#id)))?;)
        }
    });
    Ok(quote! {
        #guard
        impl ::bits::BitEncode for #name {
            fn bit_encode<K: ::bits::__private::Sink>(
                &self,
                w: &mut K,
            ) -> ::core::result::Result<(), ::bits::__private::BitError> {
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
        } else {
            return Err(meta.error(
                "unknown `#[bin(...)]` option; expected one of: read_only, write_only, \
                 no_builder, bit_order = msb|lsb, allow_byte_aligned",
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
                    <#ty as ::bits::BitDecode>::BIT_LEN % 8 == 0,
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
