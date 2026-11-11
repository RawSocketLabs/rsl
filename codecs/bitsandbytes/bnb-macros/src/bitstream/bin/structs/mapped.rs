//! `#[bin]` on a wire-mapped struct (`map`/`try_map`/`bw_map`, `wire`/`try_wire`): the
//! logical type serializes through a separate wire type.

use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{ItemStruct, Type};

use crate::bitstream::bin::args::BinArgs;
use crate::bitstream::generate::{DecodeEntryPoints, decode_layout_impl};

/// The wire **message** type for a mapped `#[bin]` struct: given directly by `wire`/`try_wire`,
/// or (closure form) taken from the `map`/`try_map` closure's annotated parameter, or the
/// `bw_map` closure's annotated return (write-only).
fn mapped_wire_type(args: &BinArgs) -> syn::Result<Type> {
    // Conversion-trait form: the wire type is named directly.
    if let Some(w) = args.wire.as_ref().or(args.try_wire.as_ref()) {
        return Ok(w.clone());
    }
    // Closure form: from the decode closure's annotated parameter…
    if let Some(expr) = args.map.as_ref().or(args.try_map.as_ref()) {
        if let syn::Expr::Closure(c) = expr {
            return match c.inputs.first() {
                Some(syn::Pat::Type(pt)) => Ok((*pt.ty).clone()),
                _ => Err(syn::Error::new_spanned(
                    expr,
                    "the `map`/`try_map` closure must annotate its wire-type parameter, \
                     e.g. `map = |w: WireType| …`",
                )),
            };
        }
        return Err(syn::Error::new_spanned(
            expr,
            "struct-level `map`/`try_map` must be a closure, e.g. `map = |w: WireType| Self::from(w)`",
        ));
    }
    // …or (write-only) the encode closure's annotated return.
    let expr = args
        .bw_map
        .as_ref()
        .expect("a mapped struct sets at least one mapping option");
    if let syn::Expr::Closure(c) = expr {
        if let syn::ReturnType::Type(_, ty) = &c.output {
            return Ok((**ty).clone());
        }
        return Err(syn::Error::new_spanned(
            expr,
            "a write-only mapped struct needs the wire type: annotate the `bw_map` return, \
             e.g. `bw_map = |s: &Self| -> WireType { … }`",
        ));
    }
    Err(syn::Error::new_spanned(
        expr,
        "struct-level `bw_map` must be a closure",
    ))
}

/// The **mapped** `#[bin]` struct path: the logical type's wire form is another message type
/// `Wire`, bridged either by closures (`map`/`try_map` decode, `bw_map` encode) or by the
/// conversion traits (`wire`/`try_wire`: `From`/`TryFrom<Wire>` decode + `From<&Self>` encode).
/// Bypasses the field codec — the struct's own fields are the *logical* data, never the wire.
/// Because the generated `BitDecode`/`BitEncode` carry the mapping, the ordinary slice helpers
/// (`decode_all`/`peek`/…) work at the wire type's layout. It does **not** emit `FixedBitLen`
/// (so the wire type may be variable-length); add `impl FixedBitLen` by hand to nest a
/// fixed-wire mapped type as a plain field.
#[allow(clippy::too_many_lines)] // Paired read/write mapping and their generated surface.
pub(super) fn bin_struct_mapped(args: &BinArgs, s: &ItemStruct) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let name = &s.ident;
    let vis = &s.vis;

    let from_form = args.wire.is_some() || args.try_wire.is_some();
    let closure_form = args.map.is_some() || args.try_map.is_some() || args.bw_map.is_some();
    if from_form && closure_form {
        return Err(syn::Error::new_spanned(
            name,
            "`wire`/`try_wire` (conversion-trait mapping) can't be combined with \
             `map`/`try_map`/`bw_map` (closure mapping) — pick one form",
        ));
    }
    if args.wire.is_some() && args.try_wire.is_some() {
        return Err(syn::Error::new_spanned(
            name,
            "`wire` and `try_wire` are mutually exclusive",
        ));
    }
    if args.map.is_some() && args.try_map.is_some() {
        return Err(syn::Error::new_spanned(
            name,
            "`map` and `try_map` are mutually exclusive",
        ));
    }
    if !s.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &s.generics,
            "#[bin] does not support generic parameters yet",
        ));
    }
    for (opt, present) in [
        ("magic", args.magic.is_some()),
        ("ctx", !args.ctx.is_empty()),
        ("validate", args.validate.is_some()),
        ("tag", args.tag.is_some()),
    ] {
        if present {
            return Err(syn::Error::new_spanned(
                name,
                format!(
                    "`{opt}` is not supported on a mapped `#[bin]` struct — put it on the wire \
                     type instead (the wire type owns the framing)"
                ),
            ));
        }
    }
    // The options a mapped struct would otherwise parse and ignore. (`no_builder` is true
    // of every item that has no builder, so it is accepted on them all.)
    args.reject(
        &["auto_len", "forward_only"],
        "is not supported on a mapped `#[bin]` struct — put it on the wire type instead (the \
         wire type owns the fields and the cursor)",
    )?;
    args.reject(
        &["big", "little", "bytes", "bits"],
        "would be ignored on a mapped `#[bin]` struct, which is read and written in its wire \
         type's own byte and bit order — put it on the wire type instead",
    )?;
    // Directions: the `wire`/`try_wire` form is bidirectional unless `read_only`/`write_only`
    // narrows it; the closure form is driven by which closures are present.
    let (has_decode, has_encode) = if from_form {
        (!args.write_only, !args.read_only)
    } else {
        (
            args.map.is_some() || args.try_map.is_some(),
            args.bw_map.is_some(),
        )
    };
    if !from_form {
        if args.read_only && has_encode {
            return Err(syn::Error::new_spanned(
                name,
                "`read_only` conflicts with `bw_map` (drop one)",
            ));
        }
        if args.write_only && has_decode {
            return Err(syn::Error::new_spanned(
                name,
                "`write_only` conflicts with `map`/`try_map` (drop one)",
            ));
        }
    }

    let wire = mapped_wire_type(args)?;
    // The slice entry points bake in the wire type's layout. An encodable mapped struct takes
    // it from the wire type's `BitEncode`, which it requires anyway. A decode-only one must
    // not require that impl, so it asks the autoref probe, once, in its own `DecodeLayout`
    // (which also makes it a wire type another decode-only mapped struct can sit on). The
    // probe carries the wire type's span: a wire type with no layout is an error there.
    let (layout, decode_layout) = if has_encode {
        (quote!(<#wire as #bnb::__private::BitEncode>::LAYOUT), None)
    } else {
        let call = quote_spanned! {wire.span()=>
            (&&#bnb::__private::WireLayoutProbe::<#wire>(::core::marker::PhantomData)).layout()
        };
        // Each wire type resolves through one of the two traits, leaving the other unused.
        let probe = quote!({
            #[allow(unused_imports)]
            use #bnb::__private::{WireLayoutDispatch as _, WireLayoutMissing as _};
            #call
        });
        (
            quote!(<#name as #bnb::__private::DecodeLayout>::layout()),
            Some(decode_layout_impl(name, &probe)),
        )
    };

    let decode_ts = if has_decode {
        let decode_call = if from_form {
            if args.wire.is_some() {
                // decode: read the wire message, then `Self::from(wire)`.
                quote!(#bnb::__private::decode_mapped_msg(
                    __bnb_r,
                    |__bnb_wire: #wire| <#name as ::core::convert::From<#wire>>::from(__bnb_wire)
                ))
            } else {
                // try_wire: `Self::try_from(wire)` — a conversion error becomes a decode error.
                quote!(#bnb::__private::decode_try_mapped_msg(
                    __bnb_r,
                    |__bnb_wire: #wire| <#name as ::core::convert::TryFrom<#wire>>::try_from(__bnb_wire)
                ))
            }
        } else if let Some(map) = &args.map {
            quote!(#bnb::__private::decode_mapped_msg(__bnb_r, #map))
        } else {
            let tm = args
                .try_map
                .as_ref()
                .expect("invariant: closure-form decode has `map` or `try_map`");
            quote!(#bnb::__private::decode_try_mapped_msg(__bnb_r, #tm))
        };
        let entry_points = DecodeEntryPoints {
            name,
            vis: &quote!(#vis),
            layout: &layout,
            cursor: &quote!(#bnb::__private::Source),
            decode_doc: "Decode one message from a `Source` cursor (mapping the wire type to this logical type).",
            decode_all_doc: "Decode every message from `bytes` into a `Vec` (at the wire type's layout).",
            decode_iter_doc: "A lazy iterator decoding successive messages from `bytes`.",
            peek_doc: "Decode one message without consuming the buffer (tail-tolerant). Not a prefix decoder: a short slice is a definitive `UnexpectedEof` — use `decode_prefix` to tell it from malformed input.",
            decode_exact_doc: "Decode and require every whole byte consumed.",
        }
        .expand();
        quote! {
            impl #bnb::__private::BitDecode for #name {
                fn bit_decode<__BnbS: #bnb::__private::Source>(
                    __bnb_r: &mut __BnbS,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #decode_call
                }
            }

            #entry_points
        }
    } else {
        quote!()
    };

    let encode_ts = if has_encode {
        let encode_call = if from_form {
            // encode: map to the wire message via `WireType::from(&self)`, then write it.
            quote!(#bnb::__private::encode_mapped_msg(
                __bnb_w,
                self,
                |__bnb_v: &#name| -> #wire { ::core::convert::Into::into(__bnb_v) }
            ))
        } else {
            let bw = args
                .bw_map
                .as_ref()
                .expect("invariant: closure-form encode has `bw_map`");
            quote!(#bnb::__private::encode_mapped_msg(__bnb_w, self, #bw))
        };
        quote! {
            impl #bnb::__private::BitEncode for #name {
                const LAYOUT: #bnb::__private::Layout = #layout;
                fn bit_encode<__BnbK: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut __BnbK,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    #encode_call
                }
            }

            impl #name {
                #[doc = "Encode to a fresh `Vec` (mapping this logical type to its wire type)."]
                #vis fn to_bytes(
                    &self,
                ) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                    #bnb::__private::encode_to_vec(self, #layout)
                }
            }
        }
    } else {
        quote!()
    };

    // No `FixedBitLen` is emitted: the wire type may be variable-length (so a variable-length
    // logical format works out of the box). To nest a *fixed*-wire mapped type as a plain field,
    // add a one-line `impl FixedBitLen for Self { const BIT_LEN = <Wire as FixedBitLen>::BIT_LEN; }`.
    Ok(quote! {
        #s
        #decode_ts
        #decode_layout
        #encode_ts
    })
}
