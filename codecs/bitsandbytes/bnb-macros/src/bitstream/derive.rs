//! The bare `#[derive(BitDecode)]` / `#[derive(BitEncode)]` front-end: the struct shape it
//! accepts, its `#[bit_stream(...)]` options, and the two entry points.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::punctuated::Punctuated;
use syn::{Data, DeriveInput, Fields, FieldsNamed, Ident, Token, parse_macro_input};

use super::ctx::CtxParam;
use super::generate::{BitStreamAttrs, gen_decode, gen_encode};

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

fn parse_bit_stream(input: &DeriveInput) -> syn::Result<BitStreamAttrs> {
    let mut attrs = BitStreamAttrs::default();
    for attr in &input.attrs {
        if attr.path().is_ident("bit_stream") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("allow_byte_aligned") {
                    attrs.allow_byte_aligned = true;
                    Ok(())
                } else if meta.path.is_ident("bits") {
                    let val: Ident = meta.value()?.parse()?;
                    match val.to_string().as_str() {
                        "msb" => attrs.lsb = false,
                        "lsb" => attrs.lsb = true,
                        _ => return Err(meta.error("expected `msb` or `lsb`")),
                    }
                    Ok(())
                } else if meta.path.is_ident("bytes") {
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
                        "unknown `#[bit_stream(...)]` option; expected `allow_byte_aligned`, `bits = msb|lsb`, `bytes = big|little`, `magic = <expr>`, or `ctx(name: Ty, …)`",
                    ))
                }
            })?;
        }
    }
    Ok(attrs)
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
    )
}

pub(crate) fn expand_encode(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match encode_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn encode_inner(input: &DeriveInput) -> syn::Result<TokenStream2> {
    // A lone derive cannot see whether `BitDecode` (and so `FixedBitLen`) is derived too.
    gen_encode(
        &input.ident,
        named_struct(input)?,
        &parse_bit_stream(input)?,
        false,
    )
}
