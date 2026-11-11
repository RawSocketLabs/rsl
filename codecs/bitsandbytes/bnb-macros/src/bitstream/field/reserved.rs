//! `#[reserved]` / `#[reserved_with(…)]` fields and their spec value.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

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
pub(in crate::bitstream) fn reserved_spec_value(
    f: &syn::Field,
) -> syn::Result<Option<TokenStream2>> {
    let bnb = crate::bnb_path();
    let ty = &f.ty;
    Ok(field_reserved(f)?.map(|reserved| match reserved {
        Reserved::Zero => quote!(<#ty as #bnb::__private::Bits>::from_bits(0)),
        Reserved::With(expr) => {
            let expr = *expr;
            quote!({ let __bnb_spec: #ty = #expr; __bnb_spec })
        }
    }))
}

/// Whether a field carries `#[reserved]`/`#[reserved_with]` (a cheap attribute check).
pub(in crate::bitstream) fn field_is_reserved(f: &syn::Field) -> bool {
    f.attrs
        .iter()
        .any(|a| a.path().is_ident("reserved") || a.path().is_ident("reserved_with"))
}
