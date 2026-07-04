//! Expansion of `#[derive(BitEnum)]`.
//!
//! Maps an enum to an integer discriminant of a fixed width (from
//! `#[bit_enum(WidthType)]`). Unit variants carry explicit or auto-incremented
//! discriminants; an optional `#[catch_all]` tuple variant captures any
//! unrecognized value, making decoding total and lossless (the dual-use
//! convention). Without a catch-all the variants must cover the whole width, or the
//! enum must be marked `#[bit_enum(.., closed)]` to assert a closed set — otherwise
//! it is a compile error, since the infallible `from_bits` (codec / `#[bitfield]`
//! getter) path would panic on an unknown discriminant.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Fields, Ident, Token, Type, parse_macro_input};

/// Parsed `#[bit_enum(WidthType, bytes = …, closed)]`. `bytes = be|le` is accepted
/// (for source compatibility) but byte order is meaningful only on the wire — a
/// `BitEnum` is a discriminant value, so it is ignored here. `closed` asserts the
/// enum is a closed set (see the exhaustiveness check in [`expand_inner`]).
struct Args {
    width: Type,
    /// `closed` — the author asserts no `#[catch_all]` is wanted even though the
    /// variants do not cover the whole width: an unknown discriminant is a contract
    /// violation (the checked `TryFrom` rejects it; the infallible path panics).
    closed: bool,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let width: Type = input.parse()?;
        let mut closed = false;
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "closed" => closed = true,
                "bytes" => {
                    input.parse::<Token![=]>()?;
                    let val: Ident = input.parse()?;
                    match val.to_string().as_str() {
                        "be" | "le" => {}
                        other => {
                            return Err(syn::Error::new_spanned(
                                &val,
                                format!("expected `be` or `le`, got `{other}`"),
                            ));
                        }
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        "expected `bytes = be|le` or `closed`",
                    ));
                }
            }
        }
        Ok(Args { width, closed })
    }
}

pub(crate) fn expand(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match expand_inner(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_inner(input: DeriveInput) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
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

    // Decode safety: `Bits::from_bits` is infallible and runs on the decode path (and
    // inside `#[bitfield]` getters), so an unknown discriminant with no `#[catch_all]`
    // has nowhere to go and panics. Require the author to either preserve unknowns
    // (catch-all, the dual-use default) or assert a closed set (`closed`) — unless the
    // unit variants already cover every value of the width, in which case the panic is
    // statically unreachable.
    if catch_all.is_none() && !args.closed {
        let covered = width_bits(width)
            .is_some_and(|bits| bits < 128 && (unit.len() as u128) == (1u128 << bits));
        if !covered {
            return Err(syn::Error::new_spanned(
                name,
                "this `BitEnum` has no `#[catch_all]` and its variants do not cover every \
                 value of its width, so decoding an unknown discriminant (on the codec path \
                 or in a `#[bitfield]` getter) would panic. Either add a catch-all variant \
                 (e.g. `#[catch_all] Other(<width>)`) to preserve unknown values — the \
                 dual-use default — or, if the set really is closed, write \
                 `#[bit_enum(<width>, closed)]` to assert it (the checked `TryFrom` still \
                 rejects unknowns; only the infallible path panics).",
            ));
        }
    }

    let bits_path = quote!(#bnb::__private::Bits);

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

    // `From`/`TryFrom` against the primitive — `num_enum` parity.
    let conv = conv_impls(name, width, &unit, catch_all.is_some());
    // The field-codec delegations, so the enum is a `#[bin]` leaf field without `#[nested]`.
    let leaf_codec = crate::bits_leaf_codec_impl(name, &bnb);

    Ok(quote! {
        impl #bits_path for #name {
            const BITS: u32 = <#width as #bits_path>::BITS;

            #[inline]
            fn into_bits(self) -> u128 {
                match self {
                    #(#into_unit,)*
                    #into_catch
                }
            }

            #[inline]
            fn from_bits(raw: u128) -> Self {
                match raw {
                    #(#from_unit,)*
                    #from_wild,
                }
            }
        }

        impl #bnb::BitEnum for #name {}

        #conv
        #leaf_codec
    })
}

/// Emits primitive interop for a byte-aligned enum — the `num_enum`
/// `IntoPrimitive`/`FromPrimitive`/`TryFromPrimitive` parity:
///
/// - `From<Enum> for uN` — always (every variant maps to a value);
/// - with a `#[catch_all]`, `From<uN> for Enum` — total, unknowns absorbed;
/// - without one, `TryFrom<uN> for Enum` — rejects unknown discriminants with
///   [`UnknownDiscriminant`](::bnb::UnknownDiscriminant).
///
/// A sub-byte width (`u4`) gets nothing — nest it in a `#[bitfield]`.
fn conv_impls(
    name: &Ident,
    width: &Type,
    unit: &[(Ident, u128)],
    has_catch_all: bool,
) -> TokenStream2 {
    if primitive_of(width).is_none() {
        return quote!(); // sub-byte width: nest it in a #[bitfield]
    }
    let bnb = crate::bnb_path();
    let bits_path = quote!(#bnb::__private::Bits);

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
                type Error = #bnb::__private::UnknownDiscriminant;
                #[inline]
                fn try_from(value: #width) -> ::core::result::Result<Self, Self::Error> {
                    match u128::from(value) {
                        #(#arms,)*
                        other => ::core::result::Result::Err(
                            #bnb::__private::UnknownDiscriminant {
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

/// The bit width of a `#[bit_enum(W)]` width type, if it is a `uN` (the `u1`..`u127`
/// aliases or the `u8`/`u16`/`u32`/`u64`/`u128` primitives). Used only to decide
/// whether the unit variants cover the whole domain.
fn width_bits(ty: &Type) -> Option<u32> {
    if let Type::Path(p) = ty {
        if let Some(seg) = p.path.segments.last() {
            if let Some(rest) = seg.ident.to_string().strip_prefix('u') {
                if let Ok(n) = rest.parse::<u32>() {
                    if (1..=128).contains(&n) {
                        return Some(n);
                    }
                }
            }
        }
    }
    None
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
