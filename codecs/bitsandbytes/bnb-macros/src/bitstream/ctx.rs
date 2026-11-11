//! The `ctx` grammar: declared parameters, passed arguments, the `…Ctx` struct's name and
//! literals, and the scan that tells whether an encode body reads a parameter.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Token, Type};

/// A declared context parameter `name: Ty` from `ctx(name: Ty, …)`.
pub(super) struct CtxParam {
    pub(super) name: Ident,
    pub(super) ty: Type,
}

impl Parse for CtxParam {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        Ok(CtxParam { name, ty })
    }
}

/// A context argument, either shorthand `name` or an alias `name = source`.
#[derive(Clone)]
pub(super) struct CtxArg {
    pub(super) name: Ident,
    pub(super) source: Ident,
}

impl Parse for CtxArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        let source = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            input.parse()?
        } else {
            name.clone()
        };
        Ok(Self { name, source })
    }
}

/// The generated context-struct name for a type/ident — `Foo` ⇒ `FooCtx`.
pub(super) fn ctx_struct_ident(name: &Ident) -> Ident {
    format_ident!("{}Ctx", name)
}

/// The context-struct **type** for a field/element type — appends `Ctx` to the
/// last path segment so `m::Value` ⇒ `m::ValueCtx`. Seen through a `macro_rules!` type
/// fragment ([`crate::ungroup`]).
pub(super) fn ctx_struct_ty(ty: &Type) -> syn::Result<TokenStream2> {
    if let Type::Path(p) = crate::ungroup(ty) {
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

/// The context-struct literal for a `ctx { a, b }` pass, resolving each name:
/// on encode a parent **field** becomes `name: self.name`, anything else (the
/// parent's own ctx param, already a local) stays shorthand `name`. On decode all
/// names are locals, so all stay shorthand.
pub(super) fn ctx_literal(
    ctx_ty: &TokenStream2,
    names: &[CtxArg],
    field_set: Option<&[&Ident]>,
) -> TokenStream2 {
    let inits = names.iter().map(|arg| {
        let n = &arg.name;
        let source = &arg.source;
        match field_set {
            Some(fields) if fields.contains(&source) => quote!(#n: self.#source),
            _ if n == source => quote!(#n),
            _ => quote!(#n: #source),
        }
    });
    quote!(#ctx_ty { #(#inits),* })
}

/// `ctx { … }` literal for a **variant** encode arm. A name that is a stored sibling is
/// a match-bound `&FieldTy`, so it is dereferenced (`*n`); a `temp` local or an enum
/// `ctx` parameter is already a value (`n`). (The struct dual, [`ctx_literal`], uses
/// `self.n` for stored fields instead.)
pub(super) fn ctx_literal_variant(
    ctx_ty: &TokenStream2,
    names: &[CtxArg],
    stored: &[&Ident],
) -> TokenStream2 {
    let inits = names.iter().map(|arg| {
        let n = &arg.name;
        let source = &arg.source;
        if stored.contains(&source) {
            quote!(#n: *#source)
        } else if n == source {
            quote!(#n)
        } else {
            quote!(#n: #source)
        }
    });
    quote!(#ctx_ty { #(#inits),* })
}

/// Whether a token stream mentions any of `names` (recursing into groups). Decides whether
/// a type's generated **encode** body reads a `ctx` parameter — and so whether `ctx` is
/// decode-only for it (plain encode) or it needs `encode_with`. Scanning the emitted tokens
/// catches *every* write-side reference (`calc`/`bw(map)`/`write_with`/`reserved_with`/
/// positioning, or a `ctx { … }` forward passing a param down) with no false negatives;
/// over-detection is harmless (it would merely keep `encode_with`).
pub(super) fn tokens_mention(ts: TokenStream2, names: &[&Ident]) -> bool {
    let tokens: Vec<_> = ts.into_iter().collect();
    let colon = |index| matches!(tokens.get(index), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == ':');
    tokens.iter().enumerate().any(|(index, tt)| match tt {
        // In `ChildCtx { target: source }`, only the source can read parent context.
        // Keep path prefixes (`name::...`) and shorthand fields as references.
        proc_macro2::TokenTree::Ident(id) => {
            (!colon(index + 1) || colon(index + 2)) && names.contains(&id)
        }
        proc_macro2::TokenTree::Group(g) => {
            if index > 0 && matches!(&tokens[index - 1], proc_macro2::TokenTree::Punct(p) if p.as_char() == '!') {
                // Macro argument syntax is opaque: `n: Type` can still expand to a read of n.
                tokens_mention_any(g.stream(), names)
            } else {
                tokens_mention(g.stream(), names)
            }
        },
        _ => false,
    })
}

/// Conservative reference scan for opaque macro arguments, including nested groups.
fn tokens_mention_any(ts: TokenStream2, names: &[&Ident]) -> bool {
    ts.into_iter().any(|tt| match tt {
        proc_macro2::TokenTree::Ident(id) => names.contains(&&id),
        proc_macro2::TokenTree::Group(g) => tokens_mention_any(g.stream(), names),
        _ => false,
    })
}
