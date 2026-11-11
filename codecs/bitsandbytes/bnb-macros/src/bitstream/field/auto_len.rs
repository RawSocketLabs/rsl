//! The `auto_len` length rules: a field's `#[bw(auto_len = …)]` target and a struct-level
//! `#[bin(auto_len(...))]` entry.

use syn::{Ident, Token};

/// The derivation target of a `#[bw(auto_len = …)]` [`WireLen`](bnb::WireLen) field — a sibling
/// field measured by element count or encoded byte length.
#[derive(Clone)]
pub(in crate::bitstream) enum AutoTarget {
    /// `count(<field>)` — the sibling's `.len()` (element count).
    Count(Ident),
    /// `bytes(<field>)` — the sibling's encoded byte length (measured by a probe encode).
    Bytes(Ident),
}

/// Parse the `count(<field>)` / `bytes(<field>)` call-expression of a `#[bw(auto_len = …)]`.
pub(super) fn parse_auto_target(expr: &syn::Expr) -> syn::Result<AutoTarget> {
    let err = || {
        syn::Error::new_spanned(
            expr,
            "expected `count(<field>)` or `bytes(<field>)` naming a sibling field",
        )
    };
    let syn::Expr::Call(call) = expr else {
        return Err(err());
    };
    let syn::Expr::Path(func) = &*call.func else {
        return Err(err());
    };
    let kind = func.path.get_ident().ok_or_else(err)?;
    if call.args.len() != 1 {
        return Err(err());
    }
    let syn::Expr::Path(arg) = &call.args[0] else {
        return Err(err());
    };
    let field = arg.path.get_ident().ok_or_else(err)?.clone();
    if kind == "count" {
        Ok(AutoTarget::Count(field))
    } else if kind == "bytes" {
        Ok(AutoTarget::Bytes(field))
    } else {
        Err(err())
    }
}

/// One entry of a struct-level `#[bin(auto_len(<field>.<nested> = count|bytes(<source>), …))]`
/// directive — a cross-struct length rule: the `<nested>` [`WireLen`](bnb::WireLen) field of
/// this struct's `<field>` derives (when `Auto`) from the sibling `<source>`.
#[derive(Clone)]
pub(in crate::bitstream) struct AutoLenSpec {
    /// The immediate field holding the nested length (e.g. `header`).
    pub(in crate::bitstream) field: Ident,
    /// The nested `WireLen` field within it (e.g. `qdcount`).
    pub(in crate::bitstream) nested: Ident,
    /// `count` or `bytes`.
    pub(in crate::bitstream) kind: Ident,
    /// The sibling field measured (e.g. `questions`).
    pub(in crate::bitstream) source: Ident,
}

impl syn::parse::Parse for AutoLenSpec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let field: Ident = input.parse()?;
        input.parse::<Token![.]>()?;
        let nested: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let kind: Ident = input.parse()?;
        if kind != "count" && kind != "bytes" {
            return Err(syn::Error::new_spanned(
                &kind,
                "expected `count(<source>)` or `bytes(<source>)`",
            ));
        }
        let content;
        syn::parenthesized!(content in input);
        let source: Ident = content.parse()?;
        Ok(AutoLenSpec {
            field,
            nested,
            kind,
            source,
        })
    }
}
