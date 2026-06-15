//! Expansion of `#[derive(BitEnum)]`.
//!
//! Maps an enum to an integer discriminant of a fixed width (from
//! `#[bit_enum(WidthType)]`). Unit variants carry explicit or auto-incremented
//! discriminants; an optional `#[catch_all]` tuple variant captures any
//! unrecognized value, making decoding total and lossless (the dual-use
//! convention). Without a catch-all the enum is assumed exhaustive.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Fields, Ident, Token, Type, parse_macro_input};

/// Parsed `#[bit_enum(WidthType, bytes = …)]`.
struct Args {
    width: Type,
    big: bool,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let width: Type = input.parse()?;
        let mut big = true;
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let val: Ident = input.parse()?;
            match (key.to_string().as_str(), val.to_string().as_str()) {
                ("bytes", "be") => big = true,
                ("bytes", "le") => big = false,
                _ => return Err(syn::Error::new_spanned(&key, "expected `bytes = be|le`")),
            }
        }
        Ok(Args { width, big })
    }
}

pub fn expand(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match expand_inner(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_inner(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;

    // Locate the `#[bit_enum(...)]` attribute.
    let attr = input
        .attrs
        .iter()
        .find(|a| a.path().is_ident("bit_enum"))
        .ok_or_else(|| syn::Error::new_spanned(name, "missing #[bit_enum(WidthType)] attribute"))?;
    let args: Args = attr.parse_args()?;
    let width = &args.width;

    let data = match &input.data {
        Data::Enum(e) => e,
        _ => {
            return Err(syn::Error::new_spanned(
                name,
                "BitEnum can only derive for enums",
            ));
        }
    };

    // Partition variants into unit (discriminant) and a single catch-all.
    let mut unit: Vec<(Ident, u128)> = Vec::new();
    let mut catch_all: Option<Ident> = None;
    let mut next: u128 = 0;

    for v in &data.variants {
        let is_catch = v.attrs.iter().any(|a| a.path().is_ident("catch_all"));
        if is_catch {
            if catch_all.is_some() {
                return Err(syn::Error::new_spanned(
                    &v.ident,
                    "only one #[catch_all] variant is allowed",
                ));
            }
            match &v.fields {
                Fields::Unnamed(f) if f.unnamed.len() == 1 => {}
                _ => {
                    return Err(syn::Error::new_spanned(
                        &v.ident,
                        "#[catch_all] must be a one-field tuple variant, e.g. `Custom(u4)`",
                    ));
                }
            }
            catch_all = Some(v.ident.clone());
            continue;
        }

        match &v.fields {
            Fields::Unit => {}
            _ => {
                return Err(syn::Error::new_spanned(
                    &v.ident,
                    "non-catch-all BitEnum variants must be unit variants",
                ));
            }
        }
        let disc = match &v.discriminant {
            Some((_, expr)) => parse_discriminant(expr)?,
            None => next,
        };
        unit.push((v.ident.clone(), disc));
        next = disc + 1;
    }

    let bits_path = quote!(::bits::__private::Bits);

    // into_bits: discriminant for unit variants; inner value for catch-all.
    let into_unit = unit.iter().map(|(id, disc)| quote!(#name::#id => #disc));
    let into_catch = catch_all
        .as_ref()
        .map(|id| quote!(#name::#id(v) => <#width as #bits_path>::into_bits(v),));

    // from_bits: match each discriminant; unknown -> catch-all or unreachable.
    let from_unit = unit.iter().map(|(id, disc)| quote!(#disc => #name::#id));
    let from_wild = match &catch_all {
        Some(id) => quote!(other => #name::#id(<#width as #bits_path>::from_bits(other))),
        None => quote!(other => unreachable!(
            "non-exhaustive BitEnum {} has no variant for discriminant {} and no #[catch_all]",
            stringify!(#name), other
        )),
    };

    // Optional binrw: only when the width is a byte-aligned primitive (a sub-byte
    // enum is only meaningful nested inside a #[bitfield]).
    let binrw = if cfg!(feature = "binrw") {
        binrw_impls(name, width, args.big)
    } else {
        quote!()
    };

    // `From`/`TryFrom` against the primitive — `num_enum` parity, feature-
    // independent (also present without binrw).
    let conv = conv_impls(name, width, &unit, catch_all.is_some());

    Ok(quote! {
        impl #bits_path for #name {
            const BITS: u32 = <#width as #bits_path>::BITS;

            fn into_bits(self) -> u128 {
                match self {
                    #(#into_unit,)*
                    #into_catch
                }
            }

            fn from_bits(raw: u128) -> Self {
                match raw {
                    #(#from_unit,)*
                    #from_wild,
                }
            }
        }

        impl ::bits::BitEnum for #name {}

        #binrw
        #conv
    })
}

/// Emits primitive interop for a byte-aligned enum — the `num_enum`
/// `IntoPrimitive`/`FromPrimitive`/`TryFromPrimitive` parity:
///
/// - `From<Enum> for uN` — always (every variant maps to a value);
/// - with a `#[catch_all]`, `From<uN> for Enum` — total, unknowns absorbed;
/// - without one, `TryFrom<uN> for Enum` — rejects unknown discriminants with
///   [`UnknownDiscriminant`](::bits::UnknownDiscriminant).
///
/// A sub-byte width (`u4`) gets nothing — nest it in a `#[bitfield]`. The
/// conversions don't need binrw, so they're emitted in both feature configs.
fn conv_impls(
    name: &Ident,
    width: &Type,
    unit: &[(Ident, u128)],
    has_catch_all: bool,
) -> TokenStream2 {
    if primitive_of(width).is_none() {
        return quote!(); // sub-byte width: nest it in a #[bitfield]
    }
    let bits_path = quote!(::bits::__private::Bits);

    // Enum -> primitive: total. `from_bits` performs the (lossless) down-cast,
    // keeping `as` out of the expanded call site.
    let into_prim = quote! {
        impl ::core::convert::From<#name> for #width {
            #[inline]
            fn from(value: #name) -> Self {
                <#width as #bits_path>::from_bits(<#name as #bits_path>::into_bits(value))
            }
        }
    };

    // primitive -> Enum: infallible `From` iff a catch-all can absorb unknowns,
    // else a checked `TryFrom`.
    let from_prim = if has_catch_all {
        quote! {
            impl ::core::convert::From<#width> for #name {
                #[inline]
                fn from(value: #width) -> Self {
                    <#name as #bits_path>::from_bits(u128::from(value))
                }
            }
        }
    } else {
        let arms = unit
            .iter()
            .map(|(id, disc)| quote!(#disc => ::core::result::Result::Ok(#name::#id)));
        quote! {
            impl ::core::convert::TryFrom<#width> for #name {
                type Error = ::bits::__private::UnknownDiscriminant;
                #[inline]
                fn try_from(value: #width) -> ::core::result::Result<Self, Self::Error> {
                    match u128::from(value) {
                        #(#arms,)*
                        other => ::core::result::Result::Err(
                            ::bits::__private::UnknownDiscriminant {
                                value: other,
                                type_name: ::core::stringify!(#name),
                            },
                        ),
                    }
                }
            }
        }
    };

    quote! {
        #into_prim
        #from_prim
    }
}

/// Emits binrw impls for an enum whose width is a byte-aligned primitive.
fn binrw_impls(name: &Ident, width: &Type, big: bool) -> TokenStream2 {
    let prim = match primitive_of(width) {
        Some(p) => p,
        None => return quote!(), // sub-byte width: nest it in a #[bitfield]
    };
    let prim = Ident::new(prim, proc_macro2::Span::call_site());
    let endian = if big { quote!(Big) } else { quote!(Little) };
    quote! {
        const _: () = {
            use ::bits::__private::binrw::{BinRead, BinResult, BinWrite, Endian};
            use ::bits::__private::binrw::io::{Read, Seek, Write};
            use ::bits::__private::binrw::meta::{EndianKind, ReadEndian, WriteEndian};
            use ::bits::__private::Bits;

            impl ReadEndian for #name { const ENDIAN: EndianKind = EndianKind::Endian(Endian::#endian); }
            impl WriteEndian for #name { const ENDIAN: EndianKind = EndianKind::Endian(Endian::#endian); }

            impl BinRead for #name {
                type Args<'a> = ();
                fn read_options<R: Read + Seek>(reader: &mut R, _endian: Endian, _args: ()) -> BinResult<Self> {
                    let raw = <#prim as BinRead>::read_options(reader, Endian::#endian, ())?;
                    Ok(<#name as Bits>::from_bits(raw as u128))
                }
            }
            impl BinWrite for #name {
                type Args<'a> = ();
                fn write_options<W: Write + Seek>(&self, writer: &mut W, _endian: Endian, _args: ()) -> BinResult<()> {
                    let raw = <#name as Bits>::into_bits(*self) as #prim;
                    <#prim as BinWrite>::write_options(&raw, writer, Endian::#endian, ())
                }
            }
        };
    }
}

/// Returns the primitive name if `ty` is `u8`/`u16`/`u32`/`u64`/`u128`.
fn primitive_of(ty: &Type) -> Option<&'static str> {
    if let Type::Path(p) = ty {
        if let Some(seg) = p.path.segments.last() {
            return match seg.ident.to_string().as_str() {
                "u8" => Some("u8"),
                "u16" => Some("u16"),
                "u32" => Some("u32"),
                "u64" => Some("u64"),
                "u128" => Some("u128"),
                _ => None,
            };
        }
    }
    None
}

fn parse_discriminant(expr: &syn::Expr) -> syn::Result<u128> {
    if let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Int(i),
        ..
    }) = expr
    {
        i.base10_parse::<u128>()
    } else {
        Err(syn::Error::new_spanned(
            expr,
            "BitEnum discriminants must be integer literals",
        ))
    }
}
