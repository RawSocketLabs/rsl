//! The `#[bin]` entry point: parses the attribute's options, then hands the item to the
//! struct or the enum front-end.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Ident, Token};

use super::args::BinArgs;
use super::enums::bin_enum;
use super::structs::bin_struct;
use crate::bitstream::ctx::CtxParam;
use crate::bitstream::field::AutoLenSpec;

// ---------------------------------------------------------------------------
// `#[bin]` — the unified codec attribute.
//
// One macro that folds codec + builder. It generates the codec **directly** from
// the full field list (shared generators: `gen_decode`/`gen_encode`, the same
// functions the bare derives call) — it does *not* lower to the derives, because
// it needs the full list (`temp` fields included) while emitting a struct without
// them. That struct ownership is also what enables field *injection*: the
// `count_prefix` desugar into the temp+calc+count triad — not possible from a
// derive, which cannot re-emit the item.
// Field directives (`#[br]`/`#[bw]`/`#[brw]`) are stripped from the emitted struct.
// ---------------------------------------------------------------------------

/// One `parse = <expr>` / `write = <expr>` entry inside `#[bin(codec(...))]`. The expr
/// swallows a full turbofish path (`prefixed::parse_string::<_, u16>` is one expression),
/// so the comma-separated entries split correctly.
struct CodecFn {
    name: Ident,
    expr: syn::Expr,
}

impl Parse for CodecFn {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        Ok(CodecFn {
            name,
            expr: input.parse()?,
        })
    }
}

/// Entry for `#[bin(...)]`.
pub(crate) fn expand_bin(attr: TokenStream, item: TokenStream) -> TokenStream {
    match bin_inner(attr, item) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[allow(clippy::too_many_lines)] // The complete attribute grammar is intentionally parsed in one place.
fn bin_inner(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream2> {
    let mut args = BinArgs::default();
    let parser = syn::meta::parser(|meta| {
        if let Some(option) = meta.path.get_ident() {
            args.seen.push((option.to_string(), option.span()));
        }
        if meta.path.is_ident("read_only") {
            args.read_only = true;
        } else if meta.path.is_ident("write_only") {
            args.write_only = true;
        } else if meta.path.is_ident("no_builder") {
            args.no_builder = true;
        } else if meta.path.is_ident("forward_only") {
            args.forward_only = true;
        } else if meta.path.is_ident("bits") {
            let v: Ident = meta.value()?.parse()?;
            match v.to_string().as_str() {
                "msb" => args.lsb = false,
                "lsb" => args.lsb = true,
                _ => return Err(meta.error("expected `msb` or `lsb`")),
            }
        } else if meta.path.is_ident("bytes") {
            let v: Ident = meta.value()?.parse()?;
            match v.to_string().as_str() {
                "big" => args.little = false,
                "little" => args.little = true,
                _ => return Err(meta.error("expected `big` or `little`")),
            }
        } else if meta.path.is_ident("big") {
            // Bare `big`/`little` is the terse `#[bin]` sugar for `bytes = big|little`.
            args.little = false;
        } else if meta.path.is_ident("little") {
            args.little = true;
        } else if meta.path.is_ident("magic") {
            args.magic = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("ctx") {
            let content;
            syn::parenthesized!(content in meta.input);
            let params = Punctuated::<CtxParam, Token![,]>::parse_terminated(&content)?;
            args.ctx = params.into_iter().map(|p| (p.name, p.ty)).collect();
        } else if meta.path.is_ident("auto_len") {
            let content;
            syn::parenthesized!(content in meta.input);
            let specs = Punctuated::<AutoLenSpec, Token![,]>::parse_terminated(&content)?;
            args.auto_len = specs.into_iter().collect();
        } else if meta.path.is_ident("validate") {
            args.validate = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("tag") {
            args.tag = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("map") {
            args.map = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("try_map") {
            args.try_map = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("bw_map") {
            args.bw_map = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("wire") {
            args.wire = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("try_wire") {
            args.try_wire = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("codec") {
            if args.has_codec() {
                return Err(meta.error("duplicate `codec`"));
            }
            if meta.input.peek(Token![=]) {
                // `codec = <module>` — shorthand for the module's `parse`/`write` pair.
                let m: syn::Path = meta.value()?.parse()?;
                args.codec_parse = Some(syn::parse_quote!(#m::parse));
                args.codec_write = Some(syn::parse_quote!(#m::write));
            } else {
                // `codec(parse = <f>, write = <f>)` — the general form (any fn names,
                // turbofish allowed); either entry may be omitted under a directional
                // struct (checked in `bin_struct_codec`).
                let content;
                syn::parenthesized!(content in meta.input);
                let fns = Punctuated::<CodecFn, Token![,]>::parse_terminated(&content)?;
                for f in fns {
                    match f.name.to_string().as_str() {
                        "parse" => {
                            if args.codec_parse.is_some() {
                                return Err(syn::Error::new_spanned(&f.name, "duplicate `parse`"));
                            }
                            args.codec_parse = Some(f.expr);
                        }
                        "write" => {
                            if args.codec_write.is_some() {
                                return Err(syn::Error::new_spanned(&f.name, "duplicate `write`"));
                            }
                            args.codec_write = Some(f.expr);
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                &f.name,
                                "unknown `codec(...)` entry; expected `parse = <fn>` and/or `write = <fn>`",
                            ));
                        }
                    }
                }
                if !args.has_codec() {
                    return Err(meta.error(
                        "empty `codec(...)`; expected `parse = <fn>` and/or `write = <fn>`",
                    ));
                }
            }
        } else {
            return Err(meta.error(
                "unknown `#[bin(...)]` option; expected one of: read_only, write_only, \
                 no_builder, forward_only, big, little, bytes = big|little, bits = msb|lsb, magic = <expr>, \
                 ctx(name: Ty, …), auto_len(<field>.<nested> = count|bytes(<source>), …), \
                 validate = <path>, tag = <ctx-param>, \
                 map/try_map = |w: Wire| …, bw_map = |s: &Self| Wire, wire/try_wire = WireType, \
                 codec = <module>, or codec(parse = <f>, write = <f>)",
            ));
        }
        Ok(())
    });
    Parser::parse(parser, attr)?;

    if args.read_only && args.write_only {
        return Err(syn::Error::new(
            ::proc_macro2::Span::call_site(),
            "`read_only` and `write_only` are mutually exclusive",
        ));
    }
    if args.has_codec() && args.is_mapped() {
        return Err(syn::Error::new(
            ::proc_macro2::Span::call_site(),
            "`codec` and struct-level wire mapping (`map`/`try_map`/`bw_map`/`wire`/`try_wire`) \
             are mutually exclusive — a codec newtype's fns own its wire form",
        ));
    }
    match syn::parse::<syn::Item>(item)? {
        syn::Item::Struct(s) => bin_struct(&args, &s),
        syn::Item::Enum(e) => {
            if args.is_mapped() {
                return Err(syn::Error::new_spanned(
                    &e.ident,
                    "struct-level wire mapping (`map`/`try_map`/`bw_map`/`wire`/`try_wire`) applies \
                     to a `#[bin]` struct, not an enum — map the enum's variant fields, or wrap \
                     the enum in a mapped struct",
                ));
            }
            if args.has_codec() {
                return Err(syn::Error::new_spanned(
                    &e.ident,
                    "`codec` applies to a single-field tuple struct (a newtype), not an enum",
                ));
            }
            bin_enum(&args, &e)
        }
        other => Err(syn::Error::new_spanned(
            other,
            "#[bin] requires a struct or an enum",
        )),
    }
}
