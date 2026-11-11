//! A single field directive: its user-facing name (`D_*`), the record of it as written, and
//! the `#[br(...)]` directive grammar.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, Token};

use crate::bitstream::ctx::CtxArg;

/// One directive as written on a field, for [`check_field_directives`].
#[derive(Clone)]
pub(super) struct Written {
    /// Which directive: one of the `D_*` names.
    pub(super) name: &'static str,
    /// The directive's keyword, where a diagnostic about it points.
    pub(super) span: proc_macro2::Span,
    /// The value's tokens (empty for a flag), to tell a restatement from a different value.
    pub(super) value: String,
}

impl Written {
    /// The directive `name` at `span`, carrying `value`.
    pub(super) fn new(
        name: &'static str,
        span: proc_macro2::Span,
        value: &dyn quote::ToTokens,
    ) -> Self {
        let value = value.to_token_stream().to_string();
        Self { name, span, value }
    }

    /// Which of the two attributes a [`D_RESERVED`] record was written as, for a diagnostic.
    pub(super) fn reserved_form(&self) -> &'static str {
        if self.value.starts_with("reserved_with") {
            "#[reserved_with(…)]"
        } else {
            D_RESERVED
        }
    }
}

// The field directives as the user writes them: the names diagnostics print and
// `FIELD_CONFLICTS` is keyed by.
pub(super) const D_COUNT: &str = "#[br(count = …)]";
pub(super) const D_CTX: &str = "#[br(ctx { … })]";
pub(super) const D_TEMP: &str = "#[br(temp)]";
pub(super) const D_IF: &str = "#[br(if(…))]";
pub(super) const D_MAP: &str = "#[br(map = …)]";
pub(super) const D_TRY_MAP: &str = "#[br(try_map = …)]";
pub(super) const D_PARSE_WITH: &str = "#[br(parse_with = …)]";
pub(super) const D_PAD_BEFORE: &str = "#[br(pad_before = …)]";
pub(super) const D_PAD_AFTER: &str = "#[br(pad_after = …)]";
pub(super) const D_ALIGN_BEFORE: &str = "#[br(align_before)]";
pub(super) const D_ALIGN_AFTER: &str = "#[br(align_after)]";
pub(super) const D_RESTORE_POSITION: &str = "#[br(restore_position)]";
pub(super) const D_SEEK: &str = "#[br(seek = …)]";
pub(super) const D_DBG: &str = "#[br(dbg)]";
pub(super) const D_ASSERT: &str = "#[br(assert(…))]";
pub(super) const D_BR_CALC: &str = "#[br(calc = …)]";
pub(in crate::bitstream) const D_BW_CALC: &str = "#[bw(calc = …)]";
pub(super) const D_BW_MAP: &str = "#[bw(map = …)]";
pub(super) const D_WRITE_WITH: &str = "#[bw(write_with = …)]";
pub(super) const D_AUTO_LEN: &str = "#[bw(auto_len = …)]";
pub(super) const D_IGNORE: &str = "#[brw(ignore)]";
pub(super) const D_VARIABLE: &str = "#[brw(variable)]";
pub(super) const D_RESERVED: &str = "#[reserved]";

/// One `#[br(...)]` directive. A hand-rolled parser (not `parse_nested_meta`)
/// because `if` is a keyword and can't be read as a meta path ident.
pub(super) enum BrDirective {
    Count(syn::Expr),
    Ctx(Vec<CtxArg>),
    Temp,
    If(syn::Expr),
    Map(syn::Expr),
    TryMap(syn::Expr),
    ParseWith(syn::Expr),
    PadBefore(syn::Expr),
    PadAfter(syn::Expr),
    AlignBefore,
    AlignAfter,
    RestorePosition,
    Seek(syn::Expr),
    Dbg,
    Assert(Box<syn::Expr>, Option<Punctuated<syn::Expr, Token![,]>>),
    Calc(syn::Expr),
}

impl BrDirective {
    /// The directive's value as written (nothing for a flag), for [`Written::value`].
    pub(super) fn value(&self) -> TokenStream2 {
        match self {
            BrDirective::Count(e)
            | BrDirective::If(e)
            | BrDirective::Map(e)
            | BrDirective::TryMap(e)
            | BrDirective::ParseWith(e)
            | BrDirective::PadBefore(e)
            | BrDirective::PadAfter(e)
            | BrDirective::Seek(e)
            | BrDirective::Calc(e) => quote!(#e),
            BrDirective::Ctx(args) => {
                let args = args
                    .iter()
                    .map(|CtxArg { name, source }| quote!(#name = #source));
                quote!(#(#args),*)
            }
            // `assert` may repeat whatever it says, so its value is never compared.
            BrDirective::Temp
            | BrDirective::AlignBefore
            | BrDirective::AlignAfter
            | BrDirective::RestorePosition
            | BrDirective::Dbg
            | BrDirective::Assert(..) => quote!(),
        }
    }

    /// The directive as the user writes it (a `D_*` name), for [`FieldBr::seen`].
    pub(super) fn name(&self) -> &'static str {
        match self {
            BrDirective::Count(_) => D_COUNT,
            BrDirective::Ctx(_) => D_CTX,
            BrDirective::Temp => D_TEMP,
            BrDirective::If(_) => D_IF,
            BrDirective::Map(_) => D_MAP,
            BrDirective::TryMap(_) => D_TRY_MAP,
            BrDirective::ParseWith(_) => D_PARSE_WITH,
            BrDirective::PadBefore(_) => D_PAD_BEFORE,
            BrDirective::PadAfter(_) => D_PAD_AFTER,
            BrDirective::AlignBefore => D_ALIGN_BEFORE,
            BrDirective::AlignAfter => D_ALIGN_AFTER,
            BrDirective::RestorePosition => D_RESTORE_POSITION,
            BrDirective::Seek(_) => D_SEEK,
            BrDirective::Dbg => D_DBG,
            BrDirective::Assert(..) => D_ASSERT,
            BrDirective::Calc(_) => D_BR_CALC,
        }
    }
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
                "calc" => {
                    input.parse::<Token![=]>()?;
                    Ok(BrDirective::Calc(input.parse()?))
                }
                "temp" => Ok(BrDirective::Temp),
                "assert" => {
                    // `assert(<expr>)` or `assert(<expr>, "fmt", args…)` — binrw parity.
                    let content;
                    syn::parenthesized!(content in input);
                    let cond: syn::Expr = content.parse()?;
                    let msg = if content.peek(Token![,]) {
                        content.parse::<Token![,]>()?;
                        Some(Punctuated::<syn::Expr, Token![,]>::parse_terminated(
                            &content,
                        )?)
                    } else {
                        None
                    };
                    Ok(BrDirective::Assert(Box::new(cond), msg))
                }
                "ignore" => Err(syn::Error::new_spanned(
                    &kw,
                    "`ignore` marks a field as neither read nor written; write it as `#[brw(ignore)]`",
                )),
                "ctx" => {
                    let content;
                    syn::braced!(content in input);
                    let names = Punctuated::<CtxArg, Token![,]>::parse_terminated(&content)?;
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
                "seek" => {
                    input.parse::<Token![=]>()?;
                    Ok(BrDirective::Seek(input.parse()?))
                }
                "dbg" => Ok(BrDirective::Dbg),
                _ => Err(syn::Error::new_spanned(
                    kw,
                    "unknown `#[br(...)]` directive; expected `count`, `calc`, `ctx`, `temp`, `if`, `assert(<expr>)`, `map`, `try_map`, `parse_with`, `pad_before/after`, `align_before/after`, `restore_position`, `seek = <bits>`, or `dbg`",
                )),
            }
        }
    }
}
