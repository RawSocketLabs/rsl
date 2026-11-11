//! The item `#[bin]` re-emits: the codec field attributes it strips, and the adaptive
//! `Debug` it substitutes for `#[try_str]` fields.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Fields, Token};

use crate::bitstream::field::field_is_temp;

/// Whether a field attribute is one `#[bin]` consumes itself (`#[nested]`/`#[br]`/
/// `#[bw]` for the codec, `#[builder]` for the builder) and must strip from the
/// struct it emits — it generates the codec and builder directly, so nothing
/// registers these as helper attributes.
pub(super) fn is_codec_field_attr(a: &syn::Attribute) -> bool {
    [
        "nested",
        "br",
        "bw",
        "brw",
        "builder",
        "reserved",
        "reserved_with",
        "try_str",
    ]
    .iter()
    .any(|n| a.path().is_ident(n))
}

/// A field marked `#[try_str]` — a `Debug`-rendering hint: render this byte-buffer field as a
/// string when it is valid UTF-8, else as hex bytes (all-or-nothing; never lossy).
pub(super) fn field_is_try_str(f: &syn::Field) -> bool {
    f.attrs.iter().any(|a| a.path().is_ident("try_str"))
}

/// Removes `Debug` from the struct's `#[derive(…)]` lists (keeping the other derives), so
/// `#[bin]` can emit a custom `Debug`. Returns whether `Debug` was present.
pub(super) fn intercept_debug_derive(
    attrs: &[syn::Attribute],
) -> syn::Result<(bool, Vec<syn::Attribute>)> {
    let mut had_debug = false;
    let mut out = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("derive") {
            let paths =
                attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)?;
            let kept: Vec<_> = paths
                .into_iter()
                .filter(|p| {
                    if p.is_ident("Debug") {
                        had_debug = true;
                        false
                    } else {
                        true
                    }
                })
                .collect();
            if !kept.is_empty() {
                out.extend(syn::Attribute::parse_outer.parse2(quote!(#[derive(#(#kept),*)]))?);
            }
        } else {
            out.push(attr.clone());
        }
    }
    Ok((had_debug, out))
}

/// The per-field `.field(…)` calls for a generated `debug_struct`, wrapping `#[try_str]` fields
/// in the adaptive [`TryStr`] formatter.
pub(super) fn debug_field_calls(
    idents: &[syn::Ident],
    try_str: &[syn::Ident],
    bnb: &TokenStream2,
) -> TokenStream2 {
    let calls = idents.iter().map(|id| {
        let id_str = crate::printed(id);
        if try_str.iter().any(|t| t == id) {
            quote!(.field(#id_str, &#bnb::__private::TryStr(&self.#id)))
        } else {
            quote!(.field(#id_str, &self.#id))
        }
    });
    quote!(#(#calls)*)
}

/// A custom `Debug` for a `#[bin]` enum when any variant field is `#[try_str]`: renders those
/// fields adaptively (string-or-bytes) and the rest as the std derive would. Returns `None` when
/// no variant field is `#[try_str]` (the std `#[derive(Debug)]` is then left untouched). It
/// matches over the **stored** fields (`temp` fields are dropped from the emitted enum).
pub(super) fn enum_try_str_debug(
    name: &syn::Ident,
    variants: &Punctuated<syn::Variant, Token![,]>,
    bnb: &TokenStream2,
) -> Option<TokenStream2> {
    let has_try_str = variants.iter().any(|v| {
        v.fields
            .iter()
            .any(|f| !field_is_temp(f) && field_is_try_str(f))
    });
    if !has_try_str {
        return None;
    }
    let arms = variants.iter().map(|v| {
        let vn = &v.ident;
        let vn_str = crate::printed(vn);
        match &v.fields {
            Fields::Unit => quote!(Self::#vn => __bnb_f.write_str(#vn_str)),
            Fields::Named(named) => {
                let stored: Vec<&syn::Field> =
                    named.named.iter().filter(|f| !field_is_temp(f)).collect();
                let binds: Vec<&syn::Ident> =
                    stored.iter().filter_map(|f| f.ident.as_ref()).collect();
                let calls = stored.iter().map(|f| {
                    let id = f.ident.as_ref().expect("invariant: a named field");
                    let id_str = crate::printed(id);
                    if field_is_try_str(f) {
                        quote!(.field(#id_str, &#bnb::__private::TryStr(#id)))
                    } else {
                        quote!(.field(#id_str, #id))
                    }
                });
                quote!(Self::#vn { #(#binds),* } =>
                    __bnb_f.debug_struct(#vn_str) #(#calls)* .finish())
            }
            Fields::Unnamed(unnamed) => {
                let stored: Vec<&syn::Field> = unnamed
                    .unnamed
                    .iter()
                    .filter(|f| !field_is_temp(f))
                    .collect();
                let binds: Vec<syn::Ident> = (0..stored.len())
                    .map(|i| quote::format_ident!("__bnb_f{i}"))
                    .collect();
                let calls = stored.iter().zip(&binds).map(|(f, b)| {
                    if field_is_try_str(f) {
                        quote!(.field(&#bnb::__private::TryStr(#b)))
                    } else {
                        quote!(.field(#b))
                    }
                });
                quote!(Self::#vn( #(#binds),* ) =>
                    __bnb_f.debug_tuple(#vn_str) #(#calls)* .finish())
            }
        }
    });
    Some(quote! {
        impl ::core::fmt::Debug for #name {
            fn fmt(&self, __bnb_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#arms),*
                }
            }
        }
    })
}
