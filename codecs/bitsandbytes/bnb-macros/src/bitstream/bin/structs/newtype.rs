//! `#[bin(codec = …)]` on a single-field tuple struct: a newtype whose wire form is owned
//! by a `parse`/`write` function pair.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Fields, ItemStruct};

use crate::bitstream::bin::args::BinArgs;
use crate::bitstream::generate::{
    BitStreamAttrs, DecodeEntryPoints, decode_layout_impl, layout_token,
};

/// The **codec newtype** `#[bin]` path: a single-field tuple struct whose wire form is
/// owned by a `parse`/`write` fn pair — `#[bin(codec = <module>)]` (the module's
/// `parse`/`write`) or `#[bin(codec(parse = <f>, write = <f>))]`. The reusable
/// *per-type* dual of the per-field `parse_with`/`write_with` escape hatch: annotate
/// once, use as a plain field everywhere. Like the mapped path it emits no
/// `FixedBitLen` (a codec's wire form is assumed variable — a fixed-width codec adds
/// the one-liner by hand); embed one in an otherwise-fixed parent with
/// `#[brw(variable)]` on the field. Also emits `From<Inner> for Self` and
/// `From<Self> for Inner` (in-memory conversions, both directions regardless of
/// `read_only`/`write_only`).
#[allow(clippy::too_many_lines)] // Paired custom-codec adapters and their generated surface.
pub(super) fn bin_struct_codec(args: &BinArgs, s: &ItemStruct) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let name = &s.ident;
    let vis = &s.vis;

    if !s.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &s.generics,
            "#[bin] does not support generic parameters yet",
        ));
    }
    // The codec fns own the framing — the whole-message options don't apply here.
    for (opt, set) in [
        ("magic", args.magic.is_some()),
        ("ctx", !args.ctx.is_empty()),
        ("validate", args.validate.is_some()),
        ("tag", args.tag.is_some()),
    ] {
        if set {
            return Err(syn::Error::new_spanned(
                &s.ident,
                format!(
                    "`{opt}` is not supported on a `#[bin(codec = …)]` newtype — the codec \
                     functions own the framing; move it into (or wrap) the codec fns, or use \
                     a full `#[bin]` struct"
                ),
            ));
        }
    }
    // The options a codec newtype would otherwise parse and ignore.
    args.reject(
        &["auto_len", "forward_only"],
        "is not supported on a `#[bin(codec = …)]` newtype — the codec functions own the \
         wire form and the cursor bound; use it on a field-coded `#[bin]` struct",
    )?;
    let inner_ty = match &s.fields {
        Fields::Unnamed(u) if u.unnamed.len() == 1 => &u.unnamed.first().expect("len checked").ty,
        _ => {
            return Err(syn::Error::new_spanned(
                &s.ident,
                "`codec` applies to a single-field tuple struct (a newtype), e.g. \
                 `#[bin(codec = bnb::codecs::leb128)] pub struct Varint(pub u64);` — for a \
                 one-off field use `#[br(parse_with = …)]`/`#[bw(write_with = …)]` instead",
            ));
        }
    };
    // Directional narrowing: an unneeded fn is silently unused (the module shorthand
    // always synthesizes both), but a *needed* one must be present.
    let (has_decode, has_encode) = (!args.write_only, !args.read_only);
    if has_decode && args.codec_parse.is_none() {
        return Err(syn::Error::new_spanned(
            &s.ident,
            "this codec has no `parse` function — add `parse = <f>` inside `codec(...)`, \
             or mark the struct `write_only`",
        ));
    }
    if has_encode && args.codec_write.is_none() {
        return Err(syn::Error::new_spanned(
            &s.ident,
            "this codec has no `write` function — add `write = <f>` inside `codec(...)`, \
             or mark the struct `read_only`",
        ));
    }

    // The newtype's own declared order backs its slice entry points (`decode_all`/
    // `to_bytes`); a *field* of this type decodes through the parent's cursor, where
    // the parent's layout governs. There is no wire type to borrow `LAYOUT` from.
    let layout = layout_token(&BitStreamAttrs {
        allow_byte_aligned: true, // inert here (no guard runs on this path)
        lsb: args.lsb,
        little: args.little,
        magic: None,
        ctx: Vec::new(),
        auto_len: Vec::new(),
    });

    let decode_ts = if has_decode {
        let parse_fn = args.codec_parse.as_ref().expect("checked above");
        let entry_points = DecodeEntryPoints {
            name,
            vis: &quote!(#vis),
            layout: &layout,
            cursor: &quote!(#bnb::__private::Source),
            decode_doc: "Decode one value from a `Source` cursor (via this type's codec functions).",
            decode_all_doc: "Decode every value from `bytes` into a `Vec` (at this type's declared layout).",
            decode_iter_doc: "A lazy iterator decoding successive values from `bytes`.",
            peek_doc: "Decode one value without consuming the buffer (tail-tolerant). Not a prefix decoder: a short slice is a definitive `UnexpectedEof` — use `decode_prefix` to tell it from malformed input.",
            decode_exact_doc: "Decode and require every whole byte consumed.",
        }
        .expand();
        quote! {
            impl #bnb::__private::BitDecode for #name {
                fn bit_decode<__BnbS: #bnb::__private::Source>(
                    __bnb_r: &mut __BnbS,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    // The codec fn's `BitError` passes through untouched: the *parent's*
                    // field read wraps it with `in_field` (innermost-wins), so wrapping
                    // here would steal the parent's field name.
                    ::core::result::Result::Ok(Self((#parse_fn)(__bnb_r)?))
                }
            }

            #entry_points
        }
    } else {
        quote!()
    };

    let encode_ts = if has_encode {
        let write_fn = args.codec_write.as_ref().expect("checked above");
        quote! {
            impl #bnb::__private::BitEncode for #name {
                const LAYOUT: #bnb::__private::Layout = #layout;
                fn bit_encode<__BnbK: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut __BnbK,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    (#write_fn)(&self.0, __bnb_w)
                }
            }

            impl #name {
                #[doc = "Encode to a fresh `Vec` (via this type's codec functions)."]
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

    // In-memory conversions, both directions regardless of the wire direction. Emitted
    // in the struct's own module scope, so a private `.0` is still reachable.
    let from_ts = quote! {
        impl ::core::convert::From<#inner_ty> for #name {
            fn from(v: #inner_ty) -> Self {
                Self(v)
            }
        }
        impl ::core::convert::From<#name> for #inner_ty {
            fn from(v: #name) -> Self {
                v.0
            }
        }
    };

    // No `FixedBitLen` is emitted: a codec's wire form is assumed variable-length. A
    // fixed-width codec opts in with the one-line manual impl (same doctrine as the
    // mapped path above); a variable one embeds in a fixed parent via `#[brw(variable)]`.
    let decode_layout = (!has_encode).then(|| decode_layout_impl(name, &layout));
    Ok(quote! {
        #s
        #decode_ts
        #decode_layout
        #encode_ts
        #from_ts
    })
}
