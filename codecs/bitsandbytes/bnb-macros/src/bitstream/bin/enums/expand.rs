//! `#[bin]` on an enum: the dispatched decode and encode, the accessors and helpers, and
//! the emitted enum.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{Fields, Ident};

use super::dispatch::{EnumDispatch, MagicWidth, VariantRole};
use super::magic::{Magic, parse_magic};
use super::variant::{Directions, variant_bind_idents, variant_field_codec};
use crate::bitstream::bin::args::BinArgs;
use crate::bitstream::bin::item::{
    enum_try_str_debug, intercept_debug_derive, is_codec_field_attr,
};
use crate::bitstream::bin::structs::field_normalizes_aliases;
use crate::bitstream::ctx::{ctx_struct_def, ctx_struct_ident, tokens_mention};
use crate::bitstream::field::{
    CountPrefixSite, desugar_count_prefix, extract_count_prefix, field_is_temp, parse_field_br,
};
use crate::bitstream::generate::{
    BitStreamAttrs, DECODE_ALL_DOC, DECODE_ITER_DOC, DecodeEntryPoints, PEEK_DOC,
    decode_layout_impl, layout_token,
};

// ---------------------------------------------------------------------------
// `#[bin]` on an enum — a dispatched tagged union.
//
// A variant is selected by its on-wire `magic` (a constant read+written), by a read-only
// `tag` selector drawn from a `ctx` param (never on the wire), or a hybrid of the two;
// each variant is a mini-struct whose fields reuse the `#[br]`/`#[bw]` grammar.
// `#[catch_all]` preserves an unknown discriminant and its payload (dual-use). Decode
// reuses `field_read_stmt` (it reads into locals, so it is variant-agnostic); encode reuses
// `field_write_stmt` with a variant `WriteSite` (the match-bound locals instead of `self`).
// ---------------------------------------------------------------------------

/// `CamelCase` → `snake_case`, for the generated `decode_as_<variant>` methods. A raw
/// identifier contributes its unprefixed name (`r#Type` → `type`): the result is spliced
/// into a longer identifier, where an `r#` would not be one.
fn snake_case(ident: &Ident) -> String {
    let s = ident.unraw().to_string();
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.char_indices() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Rejects what a `#[bin]` enum cannot honour: generic parameters, `validate` (an enum has
/// no builder to run it), and the struct-only `auto_len`.
fn reject_unsupported(args: &BinArgs, e: &syn::ItemEnum) -> syn::Result<()> {
    if !e.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &e.generics,
            "#[bin] does not support generic parameters yet",
        ));
    }
    if args.validate.is_some() {
        return Err(syn::Error::new_spanned(
            &e.ident,
            "`validate` needs the builder; a `#[bin]` enum has none",
        ));
    }
    // The struct options an enum would otherwise parse and ignore.
    args.reject(
        &["auto_len"],
        "is a `#[bin]` struct option and would be ignored on an enum — put \
         `#[bw(auto_len = count(<field>)|bytes(<field>))]` on the variant's own `WireLen` \
         field, or move the variant's fields into a `#[bin(auto_len(...))]` struct",
    )
}

/// The enum with `#[brw(count_prefix = <Ty>)]` desugared in each struct-style variant, for
/// dispatch parsing and codegen to see. Tuple variants are rejected: the injected length
/// field must be nameable, and inserting into positional binds would renumber them.
fn desugar_count_prefixes(e: &syn::ItemEnum) -> syn::Result<syn::ItemEnum> {
    let bnb = crate::bnb_path();
    let mut e_desugared = e.clone();
    for v in &mut e_desugared.variants {
        match &mut v.fields {
            Fields::Named(named) => {
                desugar_count_prefix(&bnb, &mut named.named, CountPrefixSite::Variant)?;
            }
            Fields::Unnamed(unnamed) => {
                for f in &mut unnamed.unnamed {
                    if extract_count_prefix(f)?.is_some() {
                        return Err(syn::Error::new_spanned(
                            &*f,
                            "`count_prefix` needs a named field (the injected length field \
                             must be nameable); use a struct-style variant",
                        ));
                    }
                }
            }
            Fields::Unit => {}
        }
    }
    Ok(e_desugared)
}

/// The bound on the explicit cursor of `decode` / `decode_with`: `SeekSource` when a variant
/// field seeks (`seek`/`restore_position`) or the variable-width / fallback magic path peeks
/// (`use_peek`), else `Source`. A `forward_only` enum rejects either, mirroring the struct
/// path.
fn cursor_bound(args: &BinArgs, e: &syn::ItemEnum, use_peek: bool) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let seeks = use_peek
        || e.variants
            .iter()
            .flat_map(|v| &v.fields)
            .any(|f| parse_field_br(f).is_ok_and(|br| br.restore_position || br.seek.is_some()));
    if args.forward_only && seeks {
        let reason = if use_peek {
            "variable-width / fallback magic dispatch peeks (it needs to seek)"
        } else {
            "a seeking variant field (`seek`/`restore_position`)"
        };
        return Err(syn::Error::new_spanned(
            &e.ident,
            format!("{reason} is incompatible with `#[bin(forward_only)]`"),
        ));
    }
    Ok(if seeks {
        quote!(#bnb::__private::SeekSource)
    } else {
        quote!(#bnb::__private::Source)
    })
}

/// The enum as emitted: `#[br(temp)]` variant fields dropped (read but not stored), and the
/// dispatch attrs (`#[bin(tag=…)]`, `#[catch_all]`) and codec field attrs stripped, leaving
/// an ordinary enum beside the generated impls.
fn stored_enum(e: &syn::ItemEnum) -> syn::ItemEnum {
    let strip = |f: &syn::Field| -> Option<syn::Field> {
        (!field_is_temp(f)).then(|| {
            let mut f = f.clone();
            f.attrs.retain(|a| !is_codec_field_attr(a));
            f
        })
    };
    let mut clean = e.clone();
    for v in &mut clean.variants {
        v.attrs
            .retain(|a| !(a.path().is_ident("bin") || a.path().is_ident("catch_all")));
        match &mut v.fields {
            Fields::Named(n) => n.named = n.named.iter().filter_map(&strip).collect(),
            Fields::Unnamed(u) => u.unnamed = u.unnamed.iter().filter_map(&strip).collect(),
            Fields::Unit => {}
        }
    }
    clean
}

/// The enum's alias-normalizer impl: one arm per variant, normalizing each stored field that
/// takes part ([`field_normalizes_aliases`]).
fn alias_normalizer(e: &syn::ItemEnum) -> syn::Result<TokenStream2> {
    let name = &e.ident;
    let mut normalize_arms = Vec::new();
    for variant in &e.variants {
        let mut stored = variant.fields.clone();
        match &mut stored {
            Fields::Named(n) => {
                n.named = n
                    .named
                    .iter()
                    .filter(|f| !field_is_temp(f))
                    .cloned()
                    .collect();
            }
            Fields::Unnamed(u) => {
                u.unnamed = u
                    .unnamed
                    .iter()
                    .filter(|f| !field_is_temp(f))
                    .cloned()
                    .collect();
            }
            Fields::Unit => {}
        }
        let ids = variant_bind_idents(&stored);
        let mut normalize = Vec::new();
        let mut bindings = Vec::new();
        let mut named_bindings = Vec::new();
        for (field, id) in stored.iter().zip(&ids) {
            if field_normalizes_aliases(field)? {
                normalize.push(crate::builder::normalize_field(&field.ty, &quote!(#id)));
                bindings.push(quote!(#id));
                named_bindings.push(quote!(#id));
            } else {
                bindings.push(quote!(_));
                named_bindings.push(quote!(#id: _));
            }
        }
        let vid = &variant.ident;
        let pattern = match &stored {
            Fields::Named(_) => quote!(Self::#vid { #(#named_bindings),* }),
            Fields::Unnamed(_) => quote!(Self::#vid(#(#bindings),*)),
            Fields::Unit => quote!(Self::#vid),
        };
        normalize_arms.push(quote!(#pattern => { #(#normalize)* }));
    }
    Ok(crate::builder::normalizer_impl(
        name,
        &quote!(match self { #(#normalize_arms),* }),
    ))
}

/// The `#[bin]` enum path. See the module banner above.
#[allow(clippy::too_many_lines)] // Dispatch and generated helpers use one validated variant model.
pub(in crate::bitstream::bin) fn bin_enum(
    args: &BinArgs,
    e: &syn::ItemEnum,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let name = &e.ident;
    let vis = &e.vis;
    reject_unsupported(args, e)?;

    let dirs = Directions {
        decode: !args.write_only,
        encode: !args.read_only,
    };

    let e_desugared = desugar_count_prefixes(e)?;
    let e = &e_desugared;

    // Enum-level `magic` is a leading prefix constant, verified on read / written on
    // encode before dispatch. Then parse + validate the per-variant dispatch model.
    let prefix = args.magic.as_ref().map(parse_magic).transpose()?;
    let dispatch = EnumDispatch::parse(e, args.tag.clone(), prefix)?;

    let named_selectors = dispatch.named_selectors(&args.ctx)?;
    let has_named_tags = !named_selectors.is_empty();

    // Dispatch mode + the variants that form the "nothing matched" tail.
    let has_selector = dispatch.selector.is_some() || has_named_tags;
    let magic_dispatch = dispatch
        .variants
        .iter()
        .any(|v| v.role() == VariantRole::MagicOnly);
    let fallback_variant = dispatch
        .variants
        .iter()
        .find(|v| v.role() == VariantRole::Fallback);
    let catch_variant = dispatch
        .variants
        .iter()
        .find(|v| v.role() == VariantRole::CatchAll);

    // Hybrid (some tag variants, some magic-only): the selector picks a tag variant first,
    // then unmatched selectors fall through to magic dispatch (tag priority).
    let hybrid = has_selector && magic_dispatch;
    if !has_selector && !magic_dispatch {
        return Err(syn::Error::new_spanned(
            name,
            "a `#[bin]` enum dispatches on `tag = <ctx-param>` (variant `tag`s) or per-variant `magic`s; it has neither",
        ));
    }
    if fallback_variant.is_some() && catch_variant.is_some() {
        return Err(syn::Error::new_spanned(
            name,
            "use either a typed fallback variant (no tag/magic) or a `#[catch_all]`, not both",
        ));
    }

    // Magic dispatch reads the discriminant once and matches by `==` when the magics are
    // uniform-width and there is no typed fallback; otherwise it probes byte-string
    // magics in declaration order, restoring mismatches and retaining the winner's
    // consumption. A fallback / catch-all reads the still-unconsumed bytes.
    let mixed = magic_dispatch && dispatch.magic_width() == MagicWidth::Mixed;
    let use_peek = magic_dispatch && (mixed || fallback_variant.is_some());
    if use_peek
        && dispatch.variants.iter().any(|v| {
            v.role() == VariantRole::MagicOnly
                && matches!(v.magic.as_ref().expect("magic-only"), Magic::Int { .. })
        })
    {
        return Err(syn::Error::new_spanned(
            name,
            "variable-width / fallback magic dispatch needs byte-string magics (so an unmatched discriminant can be re-read)",
        ));
    }

    // For tag dispatch: the selector ident + its `ctx` type. For magic dispatch: a
    // representative dispatching magic (drives the single-read `__bnb_m` + the catch writer).
    let selector_ty = dispatch
        .selector
        .as_ref()
        .map(|sel| {
            args.ctx
                .iter()
                .find(|(n, _)| n == sel)
                .map(|(_, t)| t.clone())
                .ok_or_else(|| {
                    syn::Error::new_spanned(
                        sel,
                        "`tag = <ctx-param>` must name a `ctx(...)` parameter",
                    )
                })
        })
        .transpose()?;
    let rep_magic = dispatch
        .variants
        .iter()
        .filter(|v| v.role() == VariantRole::MagicOnly)
        .find_map(|v| v.magic.as_ref());

    // The "nothing matched" tail: a typed fallback (parse the unconsumed bytes — no
    // capture), else a `#[catch_all]` capturing the read magic (`__bnb_m`, single-read), the
    // unmatched selector (`__bnb_other`, tag), or nothing (peek path — the magic stays in the
    // catch-all's own fields).
    let tail_variant = fallback_variant.or(catch_variant);
    let tail_capture: Option<TokenStream2> = if fallback_variant.is_some() || has_named_tags {
        None
    } else if catch_variant.is_some() {
        if use_peek {
            None
        } else if magic_dispatch {
            Some(quote!(__bnb_m))
        } else {
            Some(quote!(__bnb_other))
        }
    } else {
        None
    };

    // An accessor (`tag()`/`magic()`) reports a single discriminant; that is only
    // well-defined for a uniform-width, single-kind dispatch with no typed fallback (so
    // not mixed-width, not a fallback, and not a hybrid's two discriminant kinds).
    let gen_accessor = !mixed && fallback_variant.is_none() && !hybrid;

    // Per-variant encode arms (+ accessor arms). The tail variant (typed fallback or
    // catch-all) writes no discriminant of its own, except a single-read catch-all, which
    // writes back its captured magic.
    let mut encode_arms = Vec::new();
    let mut accessor_arms = Vec::new();
    let ctx_param_names: Vec<&Ident> = args.ctx.iter().map(|(n, _)| n).collect();
    // Whether any arm reads a ctx param, which selects `encode_with` over plain `BitEncode`.
    // A stored field the arm's pattern binds (a fallback or catch-all field, a captured
    // selector) shadows the param of the same name for the whole arm, so its mentions are
    // not context reads. A `temp` field is not in the pattern: it is bound later by its own
    // `calc`, which may read the param, so it shadows nothing.
    let mut arms_read_ctx = false;
    for v in &dispatch.variants {
        let is_tail = matches!(v.role(), VariantRole::Fallback | VariantRole::CatchAll);
        let cap = if is_tail { tail_capture.clone() } else { None };
        let (_, pat, writes) = variant_field_codec(
            name,
            v.variant,
            cap.as_ref(),
            if v.catch_all { &named_selectors } else { &[] },
            dirs,
        )?;

        let write_disc = if v.role() == VariantRole::CatchAll && cap.is_some() && magic_dispatch {
            let first = &variant_bind_idents(&v.variant.fields)[0];
            match rep_magic.expect("magic dispatch").kind() {
                0 => {
                    quote!(#bnb::__private::write_byte_array(#first, __bnb_w).map_err(|e| e.in_field("magic"))?;)
                }
                _ => {
                    quote!(#bnb::__private::Sink::write(__bnb_w, *#first).map_err(|e| e.in_field("magic"))?;)
                }
            }
        } else if let Some(m) = v.magic.as_ref().filter(|_| !is_tail) {
            m.write_const()
        } else {
            quote!()
        };
        let arm = quote!(#pat => { #write_disc #(#writes)* });
        let bound: Vec<Ident> = v
            .variant
            .fields
            .iter()
            .zip(variant_bind_idents(&v.variant.fields))
            .filter(|(f, _)| !field_is_temp(f))
            .map(|(_, id)| id)
            .collect();
        let read_names: Vec<&Ident> = ctx_param_names
            .iter()
            .copied()
            .filter(|n| !bound.contains(n))
            .collect();
        arms_read_ctx |= tokens_mention(arm.clone(), &read_names);
        encode_arms.push(arm);

        if gen_accessor && !has_named_tags {
            let acc = if is_tail {
                let first = &variant_bind_idents(&v.variant.fields)[0];
                quote!(*#first)
            } else if magic_dispatch {
                v.magic.as_ref().expect("magic variant").const_expr()
            } else {
                let tagval = v.tag.as_ref().expect("tag variant");
                quote!(#tagval)
            };
            accessor_arms.push(quote!(#pat => #acc,));
        }
    }

    // The "nothing matched" body: the tail variant (a typed fallback parsing the
    // unconsumed bytes, or a catch-all capturing the discriminant), else — a closed set —
    // an `unrecognized discriminant` error.
    let disc_field = if magic_dispatch { "magic" } else { "tag" };
    let dispatch_kind = match (has_selector, magic_dispatch) {
        (true, true) => quote!(#bnb::DispatchKind::TagOrMagic),
        (true, false) => quote!(#bnb::DispatchKind::Tag),
        _ => quote!(#bnb::DispatchKind::Magic),
    };
    let observed = match rep_magic.filter(|_| !use_peek) {
        Some(Magic::Int { .. }) => quote!(::core::option::Option::Some(
            #bnb::DispatchValue::Integer(::core::convert::From::from(__bnb_m))
        )),
        Some(Magic::Bytes(_)) => quote!(::core::option::Option::Some(
            #bnb::DispatchValue::Bytes(#bnb::__private::Vec::from(__bnb_m))
        )),
        None => quote!(::core::option::Option::None),
    };
    // Shared by decode and peek_variant. Only the closed terminal tail executes
    // this allocation; no new reads, tag conversions, or payload error wrapping.
    let name_str = crate::printed(name);
    let dispatch_error = quote!(#bnb::__private::BitError::no_matching_variant(
        ::core::any::TypeId::of::<Self>(),
        &(#name_str, #dispatch_kind),
        #observed,
        #bnb::__private::Source::bit_pos(__bnb_r),
    ).in_field(#disc_field));
    let tail_body = if let Some(tv) = tail_variant {
        let (reads, ctor, _) = variant_field_codec(
            name,
            tv.variant,
            tail_capture.as_ref(),
            if tv.catch_all { &named_selectors } else { &[] },
            dirs,
        )?;
        quote! {{
            #(#reads)*
            ::core::result::Result::Ok(#ctor)
        }}
    } else {
        quote! {{
            ::core::result::Result::Err(#dispatch_error)
        }}
    };

    // The decode dispatch. Magic: a single `__bnb_m` read + an `==` chain (uniform width, no
    // fallback), or a `match_magic` probe chain (variable width / fallback,
    // so the tail can re-read the unconsumed magic). Tag: a `match` on the selector. The
    // tail body is the final else.
    //
    // The magic block: a single `__bnb_m` read + `==` chain (uniform width), or a
    // `match_magic` chain (variable width / fallback), ending in the tail body.
    // Used directly for pure-magic dispatch, and as the selector `match`'s fall-through
    // under hybrid (tag takes priority, then magic).
    let magic_block = if !magic_dispatch {
        quote!()
    } else if use_peek {
        let mut chain = tail_body.clone();
        for v in dispatch.variants.iter().rev() {
            if v.role() == VariantRole::MagicOnly {
                let Some(Magic::Bytes(bytes)) = v.magic.as_ref() else {
                    unreachable!("peek path validated to byte-string magics");
                };
                let bytes = bytes.iter();
                let (reads, ctor, _) = variant_field_codec(name, v.variant, None, &[], dirs)?;
                chain = quote! {
                    if #bnb::__private::match_magic(__bnb_r, &[#(#bytes),*])? {
                        #(#reads)*
                        ::core::result::Result::Ok(#ctor)
                    } else #chain
                };
            }
        }
        chain
    } else {
        let read = rep_magic
            .expect("magic dispatch has a representative magic")
            .read_into(&format_ident!("__bnb_m"));
        let mut chain = tail_body.clone();
        for v in dispatch.variants.iter().rev() {
            if v.role() == VariantRole::MagicOnly {
                let c = v
                    .magic
                    .as_ref()
                    .expect("invariant: MagicOnly has a magic")
                    .const_expr();
                let (reads, ctor, _) = variant_field_codec(name, v.variant, None, &[], dirs)?;
                chain = quote! {
                    if __bnb_m == #c {
                        #(#reads)*
                        ::core::result::Result::Ok(#ctor)
                    } else #chain
                };
            }
        }
        quote!(#read #chain)
    };

    let dispatch_decode = if has_named_tags {
        let mut chain = tail_body.clone();
        for v in dispatch
            .variants
            .iter()
            .rev()
            .filter(|v| !v.named_tags.is_empty())
        {
            let conditions = named_selectors.iter().map(|(name, ty)| {
                let tag = v
                    .named_tags
                    .iter()
                    .find(|t| &t.name == name)
                    .expect("validated selector set");
                let value = tag.typed_value(ty);
                quote!(#name == #value)
            });
            let (reads, ctor, _) = variant_field_codec(name, v.variant, None, &[], dirs)?;
            chain = quote! {
                if #(#conditions)&&* {
                    #(#reads)*
                    ::core::result::Result::Ok(#ctor)
                } else #chain
            };
        }
        chain
    } else if has_selector {
        // Tag dispatch (incl. hybrid): match the selector; an unmatched selector falls to
        // the magic block (hybrid) or the tail body (pure tag).
        let sel = dispatch
            .selector
            .as_ref()
            .expect("tag dispatch has a selector");
        let mut arms = Vec::new();
        for v in &dispatch.variants {
            if matches!(v.role(), VariantRole::TagOnly | VariantRole::TagAndMagic) {
                let tagval = v
                    .tag
                    .as_ref()
                    .expect("invariant: a tag variant outside named-tag dispatch has `tag = V`");
                let verify = v
                    .magic
                    .as_ref()
                    .map(|m| m.verify(&v.variant.ident.to_string()));
                let (reads, ctor, _) = variant_field_codec(name, v.variant, None, &[], dirs)?;
                arms.push(quote! {
                    #tagval => {
                        #verify
                        #(#reads)*
                        ::core::result::Result::Ok(#ctor)
                    }
                });
            }
        }
        let else_body = if magic_dispatch {
            quote!({ #magic_block })
        } else {
            tail_body.clone()
        };
        // A pure-tag catch-all captures the unmatched selector (`__bnb_other`); a hybrid reads
        // `__bnb_m` in the magic block, so the unmatched selector is unused there.
        let catch_pat = if !magic_dispatch && tail_variant.is_some() {
            quote!(__bnb_other)
        } else {
            quote!(_)
        };
        quote!(match #sel { #(#arms)* #catch_pat => #else_body })
    } else {
        magic_block
    };

    let prefix_verify = dispatch.prefix.as_ref().map(|m| m.verify("prefix"));
    let prefix_write = dispatch.prefix.as_ref().map(Magic::write_const);

    let decode_body = quote! {
        #prefix_verify
        #dispatch_decode
    };
    let encode_body = quote! {
        #prefix_write
        match self {
            #(#encode_arms)*
        }
        ::core::result::Result::Ok(())
    };

    let from_bound = cursor_bound(args, e, use_peek)?;

    let attrs = BitStreamAttrs {
        allow_byte_aligned: true,
        lsb: args.lsb,
        little: args.little,
        magic: None,
        ctx: args.ctx.clone(),
        auto_len: Vec::new(), // struct-only (rejected above)
    };
    let layout = layout_token(&attrs);
    let (want_decode, want_encode) = (dirs.decode, dirs.encode);
    let is_ctx_type = !args.ctx.is_empty();
    let ctx_binds: Vec<TokenStream2> = args
        .ctx
        .iter()
        .map(|(n, _)| quote!(let #n = __bnb_ctx.#n;))
        .collect();

    // An inherent accessor on a dispatched enum: `tag()` (the off-wire selector this value
    // dispatches as — drives a parent's `#[bw(calc = self.body.tag())]`) for tag dispatch,
    // or `magic()` (the wire signature) for magic dispatch. Omitted when there is no
    // single discriminant to report (variable-width magic, or a typed fallback).
    let accessor_fn = if !gen_accessor {
        quote!()
    } else if has_named_tags {
        let methods = named_selectors
            .iter()
            .map(|(selector, ty)| {
                let arms = dispatch
                    .variants
                    .iter()
                    .map(|v| {
                        let (_, pat, _) = variant_field_codec(name, v.variant, None, &[], dirs)?;
                        let value = if v.catch_all {
                            quote!(*#selector)
                        } else {
                            v.named_tags
                                .iter()
                                .find(|t| &t.name == selector)
                                .expect("validated selector set")
                                .typed_value(ty)
                        };
                        Ok(quote!(#pat => #value,))
                    })
                    .collect::<syn::Result<Vec<_>>>()?;
                let doc = format!("The `{selector}` context value this variant dispatches as.");
                Ok(quote! {
                    #[doc = #doc]
                    #[allow(unused_variables)]
                    pub fn #selector(&self) -> #ty { match self { #(#arms)* } }
                })
            })
            .collect::<syn::Result<Vec<_>>>()?;
        quote!(impl #name { #(#methods)* })
    } else if magic_dispatch {
        let magic_ty = dispatch
            .uniform_magic_read_type()
            .expect("uniform magic dispatch has a read type");
        quote! {
            impl #name {
                #[doc = "The wire magic this value encodes as."]
                #[allow(unused_variables)]
                pub fn magic(&self) -> #magic_ty {
                    match self {
                        #(#accessor_arms)*
                    }
                }
            }
        }
    } else {
        let sel_ty = selector_ty
            .as_ref()
            .expect("tag dispatch has a selector type");
        quote! {
            impl #name {
                #[doc = "The selector this value dispatches as (the off-wire `tag`)."]
                #[allow(unused_variables)]
                pub fn tag(&self) -> #sel_ty {
                    match self {
                        #(#accessor_arms)*
                    }
                }
            }
        }
    };

    // Decode helpers: `decode_as_<variant>` parses the bytes as one explicit variant
    // (its magic, if any, then its payload), bypassing dispatch — handy when the variant
    // is known out of band, and for tests. `decode_tagged` feeds a tag-dispatched enum
    // its selector directly. A `ctx` enum threads the context through both.
    let ctx_name = ctx_struct_ident(name);
    let mut helper_methods = Vec::new();
    for v in &dispatch.variants {
        if v.role() == VariantRole::CatchAll || !want_decode {
            // The catch-all isn't an explicit target (use `decode`/`decode_with`), and a
            // write-only enum has no decoders at all.
            continue;
        }
        let mname = format_ident!("decode_as_{}", snake_case(&v.variant.ident));
        let (reads, ctor, _) = variant_field_codec(name, v.variant, None, &[], dirs)?;
        let magic_verify = v
            .magic
            .as_ref()
            .map(|m| m.verify(&v.variant.ident.to_string()));
        let doc = format!(
            "Decode `bytes` as the `{}` variant — its `magic` (if any) then its payload, requiring every whole byte consumed.",
            v.variant.ident
        );
        let body = quote! {
            #prefix_verify
            #magic_verify
            #(#reads)*
            ::core::result::Result::Ok(#ctor)
        };
        if is_ctx_type {
            helper_methods.push(quote! {
                #[doc = #doc]
                #[allow(unused_variables)]
                pub fn #mname(bytes: &[u8], __bnb_ctx: #ctx_name) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_exact_with(bytes, #layout, |__bnb_r| { #(#ctx_binds)* #body })
                }
            });
        } else {
            helper_methods.push(quote! {
                #[doc = #doc]
                pub fn #mname(bytes: &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_exact_with(bytes, #layout, |__bnb_r| { #body })
                }
            });
        }
    }
    // `decode_tagged(selector, bytes)` — sugar for a tag-dispatched enum whose only
    // context is the selector.
    if !magic_dispatch && !has_named_tags && want_decode && args.ctx.len() == 1 {
        let sel = dispatch
            .selector
            .as_ref()
            .expect("tag dispatch has a selector");
        let sel_ty = selector_ty
            .as_ref()
            .expect("tag dispatch has a selector type");
        helper_methods.push(quote! {
            #[doc = "Decode `bytes` with the given selector (tag), then dispatch — sugar for `decode_with_exact`."]
            pub fn decode_tagged(#sel: #sel_ty, bytes: &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                Self::decode_with_exact(bytes, #ctx_name { #sel })
            }
        });
    }
    // `peek_variant` + a `<Name>Kind` enum: identify the variant by the wire magic
    // (the dispatch decision only, no payload). Magic dispatch only — under tag dispatch
    // the caller already holds the selector.
    let kind_name = format_ident!("{}Kind", name);
    let kind_enum = if magic_dispatch && !has_selector && want_decode {
        // The "nothing matched" kind: the tail variant's kind, else an error.
        let tail_kind = tail_variant.map_or_else(
            || {
                quote!(::core::result::Result::Err(
                    #dispatch_error
                ))
            },
            |tv| {
                let k = &tv.variant.ident;
                quote!(::core::result::Result::Ok(#kind_name::#k))
            },
        );
        let decision = if use_peek {
            let mut chain = quote!({ #tail_kind });
            for v in dispatch.variants.iter().rev() {
                if v.role() == VariantRole::MagicOnly {
                    let Some(Magic::Bytes(bytes)) = v.magic.as_ref() else {
                        unreachable!("peek path validated to byte-string magics");
                    };
                    let bytes = bytes.iter();
                    let k = &v.variant.ident;
                    chain = quote!(if #bnb::__private::match_magic(__bnb_r, &[#(#bytes),*])? { ::core::result::Result::Ok(#kind_name::#k) } else #chain);
                }
            }
            chain
        } else {
            let read = rep_magic
                .expect("magic dispatch has a representative magic")
                .read_into(&format_ident!("__bnb_m"));
            let mut chain = quote!({ #tail_kind });
            for v in dispatch.variants.iter().rev() {
                if v.role() == VariantRole::MagicOnly {
                    let c = v
                        .magic
                        .as_ref()
                        .expect("invariant: MagicOnly has a magic")
                        .const_expr();
                    let k = &v.variant.ident;
                    chain = quote!(if __bnb_m == #c { ::core::result::Result::Ok(#kind_name::#k) } else #chain);
                }
            }
            quote!(#read #chain)
        };
        helper_methods.push(quote! {
            #[doc = "Identify which variant `bytes` is from the wire magic, without parsing the payload."]
            pub fn peek_variant(bytes: &[u8]) -> ::core::result::Result<#kind_name, #bnb::__private::BitError> {
                #bnb::__private::decode_peek_with(bytes, #layout, |__bnb_r| {
                    #prefix_verify
                    #decision
                })
            }
        });
        let kvars = dispatch.variants.iter().map(|v| &v.variant.ident);
        quote! {
            #[doc = "The variant kind of a value, from `peek_variant` (the dispatch decision only)."]
            #[derive(::core::clone::Clone, ::core::marker::Copy, ::core::fmt::Debug, ::core::cmp::PartialEq, ::core::cmp::Eq)]
            #vis enum #kind_name { #(#kvars),* }
        }
    } else {
        quote!()
    };
    let helpers = if helper_methods.is_empty() {
        quote!()
    } else {
        quote!(impl #name { #(#helper_methods)* })
    };

    // The codec impls. Decode: a `ctx` enum gets `decode_with` (+ `DecodeWith`); a plain
    // one gets `BitDecode` + the slice/stream entry points. Encode: `ctx` is decode-only,
    // so a `ctx` enum still gets a **plain** `BitEncode`/`to_bytes` unless its encode body
    // actually reads a ctx param (a `calc`/`bw(map)`/`ctx`-forward naming one) — then it
    // gets `encode_with`/`to_bytes_with` instead. Either way it impls `EncodeWith` so a
    // parent can forward to it.
    let enum_encode_uses_ctx =
        is_ctx_type && (arms_read_ctx || tokens_mention(quote!(#prefix_write), &ctx_param_names));

    let decode = if !want_decode {
        quote!()
    } else if is_ctx_type {
        quote! {
            impl #name {
                #[doc = "Decode from a bit source, given the context this type declares via `ctx(...)`."]
                #[allow(unused_variables)]
                pub fn decode_with<__BnbS: #from_bound>(
                    __bnb_r: &mut __BnbS,
                    __bnb_ctx: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #(#ctx_binds)*
                    #decode_body
                }
                #[doc = "Decode from bytes with context, requiring every whole byte consumed."]
                pub fn decode_with_exact(
                    bytes: &[u8],
                    __bnb_ctx: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_exact_with(bytes, #layout, |__bnb_r| Self::decode_with(__bnb_r, __bnb_ctx.clone()))
                }
            }
            impl #bnb::DecodeWith<#ctx_name> for #name {
                fn decode_with<__BnbS: #bnb::__private::Source>(
                    __bnb_r: &mut __BnbS,
                    args: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    <#name>::decode_with(__bnb_r, args)
                }
            }
        }
    } else {
        let entry_points = DecodeEntryPoints {
            name,
            vis: &quote!(pub),
            layout: &layout,
            cursor: &from_bound,
            decode_doc: "Decode one message from a bit cursor (a `BitReader`, `BufSource`, `BitBuf`, a streaming reader, …), advancing it; a seekable cursor is required if a variant seeks. The byte/bit order is the cursor's — use `decode_exact`/`decode_all` to bake the message's in.",
            decode_all_doc: DECODE_ALL_DOC,
            decode_iter_doc: DECODE_ITER_DOC,
            peek_doc: PEEK_DOC,
            decode_exact_doc: "Decode and require every whole byte consumed.",
        }
        .expand();
        quote! {
            impl #bnb::BitDecode for #name {
                fn bit_decode<__BnbS: #bnb::__private::Source>(
                    __bnb_r: &mut __BnbS,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #decode_body
                }
            }
            #entry_points
        }
    };

    let encode = if !want_encode {
        quote!()
    } else if enum_encode_uses_ctx {
        quote! {
            impl #name {
                #[doc = "Encode to a bit sink, given the context this type declares via `ctx(...)`."]
                #[allow(unused_variables)]
                pub fn encode_with<__BnbK: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut __BnbK,
                    __bnb_ctx: #ctx_name,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    #(#ctx_binds)*
                    #encode_body
                }
                #[doc = "Encode to a `Vec<u8>` with context."]
                pub fn to_bytes_with(
                    &self,
                    __bnb_ctx: #ctx_name,
                ) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                    #bnb::__private::encode_to_vec_with(#layout, |__bnb_w| self.encode_with(__bnb_w, __bnb_ctx.clone()))
                }
            }
            impl #bnb::EncodeWith<#ctx_name> for #name {
                fn encode_with<__BnbK: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut __BnbK,
                    args: #ctx_name,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    <#name>::encode_with(self, __bnb_w, args)
                }
            }
        }
    } else {
        // Plain encode (ctx not used on the write side). A `ctx` enum additionally impls a
        // context-ignoring `EncodeWith` so a parent can forward to it uniformly.
        let encode_with_trait = is_ctx_type.then(|| {
            quote! {
                impl #bnb::EncodeWith<#ctx_name> for #name {
                    #[allow(unused_variables)]
                    fn encode_with<__BnbK: #bnb::__private::Sink>(
                        &self,
                        __bnb_w: &mut __BnbK,
                        args: #ctx_name,
                    ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                        <Self as #bnb::BitEncode>::bit_encode(self, __bnb_w)
                    }
                }
            }
        });
        quote! {
            impl #bnb::BitEncode for #name {
                const LAYOUT: #bnb::Layout = #layout;
                // A catch-all's captured selectors are bound by the arm but never written.
                #[allow(unused_variables)]
                fn bit_encode<__BnbK: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut __BnbK,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    #encode_body
                }
            }
            impl #name {
                #[doc = "Encode to a `Vec<u8>`. To write into an explicit bit sink (a `BitWriter`)"]
                #[doc = "or a `std::io::Write`, bring `BitEncode` /"]
                #[doc = "`EncodeExt` into scope and call `.bit_encode(&mut sink)` /"]
                #[doc = "`.encode(&mut w)` (the latter is `std`-only)."]
                pub fn to_bytes(&self) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                    #bnb::__private::encode_to_vec(self, #layout)
                }
            }
            #encode_with_trait
        }
    };

    // As for a struct: a decode-only enum carries its layout without a `BitEncode`.
    let decode_layout =
        (want_decode && !want_encode && !is_ctx_type).then(|| decode_layout_impl(name, &layout));

    let ctx_struct = ctx_struct_def(vis, name, &args.ctx);

    let mut clean = stored_enum(e);

    // `#[try_str]` variant fields render adaptively: intercept `Debug` (when derived) and emit a
    // custom impl over the variants. No `#[derive(Debug)]` ⇒ nothing to replace.
    let try_str_debug = match enum_try_str_debug(name, &e.variants, &bnb) {
        Some(impl_) => {
            let (had_debug, new_attrs) = intercept_debug_derive(&clean.attrs)?;
            had_debug.then(|| {
                clean.attrs = new_attrs;
                impl_
            })
        }
        None => None,
    };

    let normalizer = alias_normalizer(e)?;

    Ok(quote! {
        #ctx_struct
        #clean
        #normalizer
        #try_str_debug
        #kind_enum
        #accessor_fn
        #helpers
        #decode
        #decode_layout
        #encode
    })
}
