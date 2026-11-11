//! `#[bin]` on a named-field struct: the field codec, the builder, and the emitted struct.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Fields, ItemStruct};

use super::mapped::bin_struct_mapped;
use super::newtype::bin_struct_codec;
use crate::bitstream::bin::args::BinArgs;
use crate::bitstream::bin::item::{
    debug_field_calls, field_is_try_str, intercept_debug_derive, is_codec_field_attr,
};
use crate::bitstream::ctx::ctx_struct_def;
use crate::bitstream::field::{
    CountPrefixSite, desugar_count_prefix, field_is_temp, parse_field_br, reserved_spec_value,
};
use crate::bitstream::generate::{
    BitStreamAttrs, decode_layout_impl, gen_decode, gen_encode, layout_token,
};

/// The `#[bin]` struct path: the codec (`BitDecode`/`BitEncode`) and the
/// required-by-default builder, folded over a named-field struct.
/// Only ordinary stored codec fields participate. Callbacks may distinguish Rust
/// variants with the same discriminant; logical/calculated data is not ours to rewrite.
pub(crate) fn field_normalizes_aliases(field: &syn::Field) -> syn::Result<bool> {
    let br = parse_field_br(field)?;
    Ok(!(br.temp
        || br.ignore
        || br.calc.is_some()
        || br.br_calc.is_some()
        || br.map.is_some()
        || br.try_map.is_some()
        || br.bw_map.is_some()
        || br.parse_with.is_some()
        || br.write_with.is_some()))
}

#[allow(clippy::too_many_lines)] // One front-end owns emitted fields, codec, builder, and validation.
pub(in crate::bitstream::bin) fn bin_struct(
    args: &BinArgs,
    s: &ItemStruct,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    if args.is_mapped() {
        return bin_struct_mapped(args, s);
    }
    if args.has_codec() {
        return bin_struct_codec(args, s);
    }
    if args.tag.is_some() {
        return Err(syn::Error::new_spanned(
            &s.ident,
            "`tag` (variant dispatch) applies to a `#[bin]` enum, not a struct",
        ));
    }
    if !s.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &s.generics,
            "#[bin] does not support generic parameters yet",
        ));
    }
    // Desugar `#[brw(count_prefix = <Ty>)]` into the temp+calc+count triad before any
    // generator sees the fields (a tuple struct no-ops here and errors just below).
    let mut s_desugared = s.clone();
    if let Fields::Named(named) = &mut s_desugared.fields {
        desugar_count_prefix(&bnb, &mut named.named, CountPrefixSite::Struct)?;
    }
    let s = &s_desugared;
    let Fields::Named(full_fields) = &s.fields else {
        return Err(syn::Error::new_spanned(
            &s.ident,
            "#[bin] requires a struct with named fields",
        ));
    };
    // `forward_only` pins a `Source`-only bound: a seek directive is then a compile
    // error (it would need a `SeekSource`).
    if args.forward_only {
        for f in &full_fields.named {
            let Ok(br) = parse_field_br(f) else { continue };
            let seeking = if br.restore_position {
                Some("restore_position")
            } else if br.seek.is_some() {
                Some("seek = …")
            } else {
                None
            };
            if let Some(name) = seeking {
                return Err(syn::Error::new_spanned(
                    f,
                    format!(
                        "`#[br({name})]` needs to seek, but the struct is `#[bin(forward_only)]`"
                    ),
                ));
            }
        }
    }

    // `#[bin]` generates the codec **directly** from the full field list — so a
    // `#[br(temp)]` field (read into a local, not stored) can participate — while
    // the emitted struct drops it. (Unlike P2.0–P2.3, this no longer lowers to the
    // `#[derive(BitDecode/BitEncode)]` codec; those derives remain usable directly.)
    //
    // The right-tool guard is always suppressed for `#[bin]`: it is the *unified*
    // codec, so a byte-aligned message is a first-class use, not a misuse. The
    // guard stays on the bare derives as advisory steering toward `#[bin]`.
    let attrs = BitStreamAttrs {
        allow_byte_aligned: true,
        lsb: args.lsb,
        little: args.little,
        magic: args.magic.clone(),
        ctx: args.ctx.clone(),
        auto_len: args.auto_len.clone(),
    };
    // `#[try_str]` is a Debug-rendering hint: a stored byte-buffer field renders as a string
    // when it is valid UTF-8, else as hex bytes. Collect the stored (non-`temp`) ones.
    let try_str_idents: Vec<syn::Ident> = full_fields
        .named
        .iter()
        .filter(|f| field_is_try_str(f) && !field_is_temp(f))
        .filter_map(|f| f.ident.clone())
        .collect();

    let decode = if args.write_only {
        quote!()
    } else {
        gen_decode(&s.ident, full_fields, &attrs)?
    };
    let encode = if args.read_only {
        quote!()
    } else {
        gen_encode(&s.ident, full_fields, &attrs, !args.write_only)?
    };
    // A decode-only message has no `BitEncode` to carry its layout (a `ctx` one has no
    // `BitDecode` either, so cannot be a wire type).
    let decode_layout = (args.read_only && args.ctx.is_empty())
        .then(|| decode_layout_impl(&s.ident, &layout_token(&attrs)));

    // The emitted struct: drop `#[br(temp)]` fields (not stored) and strip codec-only
    // field attributes (they are not registered helper attrs here — the codec is
    // generated directly, not via the derives). A `#[reserved]` field is kept (it is a
    // normal stored field now), with its `#[reserved]`/`#[reserved_with]` attr stripped.
    let mut clean = s.clone();
    if let Fields::Named(named) = &mut clean.fields {
        named.named = named
            .named
            .iter()
            .filter(|f| !field_is_temp(f))
            .cloned()
            .map(|mut f| {
                f.attrs.retain(|a| !is_codec_field_attr(a));
                f
            })
            .collect();
    }

    // `#[try_str]` fields need adaptive Debug rendering: intercept *only* `Debug` (other
    // derives stay) and emit a custom impl over all stored fields. With no `#[derive(Debug)]`
    // there's nothing to intercept.
    let mode_extras = if try_str_idents.is_empty() {
        quote!()
    } else {
        let (had_debug, new_attrs) = intercept_debug_derive(&clean.attrs)?;
        if had_debug {
            clean.attrs = new_attrs;
            let name = &s.ident;
            let name_str = crate::printed(name);
            let stored_idents: Vec<syn::Ident> = match &clean.fields {
                Fields::Named(n) => n.named.iter().filter_map(|f| f.ident.clone()).collect(),
                _ => Vec::new(),
            };
            let debug_calls = debug_field_calls(&stored_idents, &try_str_idents, &bnb);
            quote! {
                impl ::core::fmt::Debug for #name {
                    fn fmt(&self, __bnb_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        __bnb_f.debug_struct(#name_str)
                            #debug_calls
                            .finish()
                    }
                }
            }
        } else {
            quote!()
        }
    };

    // The builder is generated directly from the stored fields (so it can run the
    // `validate` hook via `builder::generate`'s post_build). `temp` fields are absent;
    // a reserved field is present but optional, defaulting to its spec value.
    let builder = if args.read_only || args.no_builder {
        if args.validate.is_some() {
            return Err(syn::Error::new_spanned(
                &s.ident,
                "`validate` needs the builder; it is incompatible with `read_only`/`no_builder`",
            ));
        }
        quote!()
    } else {
        // `validate`: run the soundness check on the built value; a failure is a
        // `BuilderError::Invalid`. The parser stays permissive (decode never runs it).
        let post_build = args.validate.as_ref().map(|path| {
            quote! {
                // `format!` through the runtime's re-export: `.to_string()` needs the
                // `ToString` prelude, which a `#![no_std]` consumer does not have.
                (#path)(&__bnb_value).map_err(|__bnb_e| {
                    #bnb::BuilderError::invalid(#bnb::__private::format!("{}", __bnb_e))
                })?;
            }
        });
        let mut bfields = Vec::new();
        for f in &full_fields.named {
            if field_is_temp(f) {
                continue; // a temp field is not stored, so not a builder field
            }
            let ident = f.ident.clone().expect("named field");
            let ty = f.ty.clone();
            // A reserved field is optional, defaulting to its spec value (so the builder
            // doesn't require it, but a caller can override it). A normal field is
            // required unless it carries `#[builder(default[= …])]`.
            let mut default = match reserved_spec_value(f)? {
                Some(spec) => crate::builder::FieldDefault::DefaultExpr(syn::parse2(spec)?),
                None => crate::builder::FieldDefault::Required,
            };
            // A `#[bw(auto_len = …)]` `WireLen` field is optional too, defaulting to
            // `WireLen::auto()` — auto-derivation is the norm, so the builder omits it; a
            // deliberate override passes `WireLen::set(n)` explicitly.
            if parse_field_br(f)?.auto_len.is_some() {
                default = crate::builder::FieldDefault::DefaultExpr(
                    syn::parse_quote!(#bnb::__private::WireLen::auto()),
                );
            }
            for attr in &f.attrs {
                if let Some(d) = crate::builder::parse_builder_attr(attr)? {
                    default = d;
                }
            }
            bfields.push(crate::builder::BField { ident, ty, default });
        }
        crate::builder::generate(
            &s.ident,
            &s.vis,
            &bfields,
            crate::builder::BuildKind::Plain,
            post_build.as_ref(),
        )?
    };

    // `ctx(...)`: the single front-end owns the generated `<Name>Ctx` struct.
    let ctx_struct = ctx_struct_def(&s.vis, &s.ident, &args.ctx);

    // `validate = path` is also exposed as re-runnable methods: `build()` checks once at
    // construction, but the value can be mutated before sending — `validate()`/`is_valid()`
    // re-check on demand (always current, never a stale flag).
    let validate_methods = if let Some(path) = &args.validate {
        let vis = &s.vis;
        let name = &s.ident;
        quote! {
            impl #name {
                #[doc = "Re-run the `#[bin(validate = …)]` soundness check against the current value."]
                #[doc = "`build()` runs it once at construction; call this before sending if the value"]
                #[doc = "may have been mutated since. It checks **semantic** soundness — by convention"]
                #[doc = "not `calc`/`reserved` fields, which are representational (normalized by"]
                #[doc = "`to_canonical_bytes`) rather than a matter of validity."]
                #[doc = ""]
                #[doc = "# Errors"]
                #[doc = "The validator's error when the value is unsound."]
                #vis fn validate(&self) -> ::core::result::Result<(), impl ::core::fmt::Display> {
                    (#path)(self)
                }
                #[doc = "Whether the `#[bin(validate = …)]` check passes for the current value —"]
                #[doc = "computed on demand, so it never goes stale after a mutation."]
                #[must_use]
                #vis fn is_valid(&self) -> bool {
                    self.validate().is_ok()
                }
            }
        }
    } else {
        quote!()
    };

    let mut normalize = Vec::new();
    for field in &full_fields.named {
        if field_normalizes_aliases(field)? {
            let id = &field.ident;
            normalize.push(crate::builder::normalize_field(
                &field.ty,
                &quote!(&mut self.#id),
            ));
        }
    }
    let normalizer = crate::builder::normalizer_impl(&s.ident, &quote!(#(#normalize)*));

    Ok(quote! {
        #ctx_struct
        #clean
        #normalizer
        #mode_extras
        #validate_methods
        #builder
        #decode
        #decode_layout
        #encode
    })
}
