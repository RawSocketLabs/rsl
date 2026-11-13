//! The encode generator: `WriteSite`, one field's write statement (with the `auto_len`
//! probe), and `gen_encode` (the `BitEncode` impl or `encode_with`, plus `to_bytes` and the
//! canonical form) over a field list.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{FieldsNamed, Ident};

use super::layout::layout_token;
use super::options::BitStreamAttrs;
use super::positioning::pad_write_tokens;
use super::width::{alignment_guard, br_indeterminate, fixed_len};
use crate::bitstream::ctx::{
    CtxArg, ctx_literal, ctx_literal_variant, ctx_struct_ident, ctx_struct_ty, tokens_mention,
};
use crate::bitstream::field::{
    AutoLenSpec, AutoTarget, FieldBr, byte_array_len, field_is_reserved, option_elem,
    parse_field_br, reserved_spec_value, vec_elem,
};

/// Where an encode statement finds its message's fields: a struct through `self`, an enum
/// variant through the locals its match arm binds (each a `&FieldTy`). The one seam between
/// the struct and variant write paths — everything else in [`field_write_core`] is shared.
#[derive(Clone, Copy)]
pub(in crate::bitstream) struct WriteSite<'a> {
    /// Whether this is an enum variant's match arm (else a struct's `self`).
    pub(in crate::bitstream) variant: bool,
    /// The stored (non-`temp`) fields, for resolving a `ctx { … }` pass.
    pub(in crate::bitstream) stored: &'a [&'a Ident],
    /// Every field with its directives, in declaration order — for a `bytes(<sibling>)`
    /// probe, which replays the sibling's own write.
    pub(in crate::bitstream) fields: &'a [(&'a syn::Field, &'a FieldBr)],
}

impl WriteSite<'_> {
    /// The field as a method receiver: `self.f`, or the bound `f` (auto-deref'd).
    fn place(self, id: &Ident) -> TokenStream2 {
        if self.variant {
            quote!(#id)
        } else {
            quote!(self.#id)
        }
    }

    /// The field as a `&FieldTy`: `&self.f`, or the bound `f`.
    fn by_ref(self, id: &Ident) -> TokenStream2 {
        if self.variant {
            quote!(#id)
        } else {
            quote!(&self.#id)
        }
    }

    /// The (`Copy`) field by value: `self.f`, or `*f`.
    fn value(self, id: &Ident) -> TokenStream2 {
        if self.variant {
            quote!(*#id)
        } else {
            quote!(self.#id)
        }
    }

    /// The `ctx { … }` literal for a field of this message ([`ctx_literal`] or its
    /// variant dual).
    fn ctx_literal(self, ctx_ty: &TokenStream2, names: &[CtxArg]) -> TokenStream2 {
        if self.variant {
            ctx_literal_variant(ctx_ty, names, self.stored)
        } else {
            ctx_literal(ctx_ty, names, Some(self.stored))
        }
    }
}

/// The encode statement for one field, wrapped with any `pad_*`/`align_*`. `Vec<T>`
/// writes every element; the count is implied by `len()` (a separate length field
/// is the user's, often `calc`'d). `site` says how the message's fields are named
/// (a struct's `self.f`, or an enum variant arm's bound `f`).
pub(in crate::bitstream) fn field_write_stmt(
    f: &syn::Field,
    br: &FieldBr,
    site: WriteSite<'_>,
    spec: bool,
) -> syn::Result<TokenStream2> {
    let pre = pad_write_tokens(br.align_before, br.pad_before.as_ref());
    let post = pad_write_tokens(br.align_after, br.pad_after.as_ref());
    let core = field_write_core(f, br, site, spec)?;
    Ok(quote!(#pre #core #post))
}

/// The encoded byte length of the sibling `source`, measured where the field `at` (the
/// `auto_len` length, or the field holding it) is written: `source`'s **own** write statement
/// run against a scratch sink, so whatever shapes its bytes (`write_with`, `bw(map)`, `ctx`,
/// `if`, a `Vec`, a `count_prefix`) shapes the measure identically.
///
/// The probe runs before the fields after `at` are written, so it first binds the `temp`
/// values declared between `at` and `source` (a tag in a length-type-value layout, say) for
/// the source's write to read, exactly as the real encode will have by then.
fn bytes_probe(
    at: &Ident,
    source: &Ident,
    site: WriteSite<'_>,
    spec: bool,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let position = |id: &Ident| {
        site.fields
            .iter()
            .position(|(f, _)| f.ident.as_ref() == Some(id))
    };
    let Some(index) = position(source) else {
        return Err(syn::Error::new_spanned(
            source,
            format!("`auto_len` measures `{source}`, which is not a field of this message"),
        ));
    };
    let (f, br) = site.fields[index];
    // A `count_prefix` source is two wire fields, the injected count and the elements; the
    // measure covers both, so the count's own write is replayed too.
    let prefix_id = format_ident!("__bnb_count_{}", source);
    let prefix_write = match position(&prefix_id) {
        Some(prefix) => {
            let (f, br) = site.fields[prefix];
            field_write_core(f, br, site, spec)?
        }
        None => quote!(),
    };
    let between = site
        .fields
        .get(position(at).map_or(index, |at| at + 1)..index)
        .unwrap_or(&[]);
    let temps = between
        .iter()
        .filter(|(f, _)| f.ident.as_ref() != Some(&prefix_id))
        .filter_map(|(f, br)| {
            let calc = br.calc.as_ref().filter(|_| br.temp)?;
            let (id, ty) = (f.ident.as_ref()?, &f.ty);
            Some(quote!(let #id: #ty = #calc;))
        });
    // A source that is itself an `auto_len` length is probed as stored, so a chained rule
    // keeps failing loudly on an unresolved `Auto` (and two lengths cannot recurse).
    let mut stored = br.clone();
    stored.auto_len = None;
    let write = field_write_core(f, &stored, site, spec)?;
    Ok(quote! {{
        // A probe encode: the byte length is independent of the writer's order.
        let mut __bnb_probe = #bnb::__private::BitWriter::new();
        {
            let __bnb_w = &mut __bnb_probe;
            #(#temps)*
            #prefix_write
            #write
        }
        __bnb_probe.bit_len().div_ceil(8)
    }})
}

/// The core encode statement (without positioning) — the single write generator for struct
/// fields and enum-variant fields alike (`site` names the field values). `spec` selects the
/// canonical pass; an enum has none, so its variants always pass `false`.
#[allow(clippy::too_many_lines)] // One ordered dispatch over mutually exclusive wire directives.
fn field_write_core(
    f: &syn::Field,
    br: &FieldBr,
    site: WriteSite<'_>,
    spec: bool,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let id = f.ident.as_ref().expect("named field");
    let id_str = crate::printed(id);
    let ty = &f.ty;
    let (place, by_ref, value) = (site.place(id), site.by_ref(id), site.value(id));
    // A `restore_position` field is a read-side peek (it overlaps later data), so it
    // is not written — the overlapping field emits those bytes.
    if br.restore_position {
        return Ok(quote!());
    }
    // `#[reserved]` on the **canonical** path: write the spec value (type zero, or the
    // `reserved_with` expression). On the verbatim (default) path a reserved field falls
    // through and writes its stored value like any field.
    if spec {
        if let Some(value) = reserved_spec_value(f)? {
            return Ok(quote!(#bnb::__private::Sink::write(__bnb_w, #value)
                .map_err(|e| e.in_field(#id_str))?;));
        }
    }
    // `ignore`: in-memory only — emit nothing.
    if br.ignore {
        return Ok(quote!());
    }
    // `#[br(calc)]`: a decode-computed field whose bits live in the raw fields it
    // derives from — it is never on the wire, so it writes nothing (both passes).
    if br.br_calc.is_some() {
        return Ok(quote!());
    }
    // `calc`: a value computed from the other fields. On the **canonical** path
    // (`spec == true`) we recompute it; on the default **verbatim** path a *stored*
    // (non-`temp`) `calc` field is written as-is — `to_bytes` never silently rewrites what
    // the caller put in the field (dual-use). A `temp` field has no stored value, so it
    // always recomputes.
    if let Some(calc) = &br.calc {
        if br.temp {
            // Not stored, so a later `#[br(ctx { … })]` pass can't resolve it via
            // `self.#id`. Bind the computed value to a **named** local (in encode-fn scope,
            // in declaration order) so the ctx pass finds it — e.g. a tag recomputed with
            // `#[bw(calc = self.body.tag())]` and handed to a `tag`-dispatched enum.
            return Ok(quote! {
                let #id: #ty = #calc;
                #bnb::__private::Sink::write(__bnb_w, #id)
                    .map_err(|e| e.in_field(#id_str))?;
            });
        }
        if spec {
            // Canonical: recompute from the other fields, pinned to the declared type.
            return Ok(quote! {
                {
                    let __bnb_calc: #ty = #calc;
                    #bnb::__private::Sink::write(__bnb_w, __bnb_calc)
                        .map_err(|e| e.in_field(#id_str))?;
                }
            });
        }
        // Verbatim (default): write the stored value exactly as it is.
        return Ok(quote!(#bnb::__private::Sink::write(__bnb_w, #value)
            .map_err(|e| e.in_field(#id_str))?;));
    }
    // A `temp` field is never stored, so it cannot be written without a `calc`.
    if br.temp {
        return Err(syn::Error::new_spanned(
            f,
            "a `#[br(temp)]` field is not stored, so it needs `#[bw(calc = <expr>)]` to encode",
        ));
    }
    // `map`: write `f(&self.field)` (the wire value). A read-side `map`/`try_map`
    // needs the inverse `#[bw(map = …)]` to be encodable.
    if let Some(bw_map) = &br.bw_map {
        return Ok(
            quote!(#bnb::__private::write_mapped(__bnb_w, #by_ref, #bw_map)
            .map_err(|e| e.in_field(#id_str))?;),
        );
    }
    // `write_with`: the escape hatch — a custom `f(&self.field, w) -> Result<()>`.
    if let Some(f) = &br.write_with {
        return Ok(quote!((#f)(#by_ref, __bnb_w)
            .map_err(|e| e.in_field(#id_str))?;));
    }
    // `auto_len = count(x)|bytes(x)`: a `WireLen<T>` field. `Set(n)` writes `n` verbatim
    // (dual-use); `Auto` derives its length from the named sibling — the element count
    // (`count`) or the encoded byte length (`bytes`, a probe encode) — checked via
    // `CountPrefix::try_from_count` (no silent truncation). `resolve_count` folds both:
    // it fills an `Auto` with the checked length and passes a `Set` through unchanged, so
    // the resolved `WireLen` (now always `Set`) writes through its own `BitEncode`. This
    // fires on both the verbatim and canonical passes — an `Auto` has no stored scalar to
    // preserve (like a `temp` `calc`), so deriving it on the verbatim path is correct.
    if let Some(target) = &br.auto_len {
        let measure = match target {
            AutoTarget::Count(field) => {
                let source = site.place(field);
                quote!(#source.len())
            }
            AutoTarget::Bytes(field) => bytes_probe(id, field, site, spec)?,
        };
        // Only an `Auto` value computes the measure (a `bytes(x)` probe encode is not
        // free); a `Set(n)` override is written verbatim, skipping the measure entirely.
        return Ok(quote! {
            {
                let __bnb_resolved = match #by_ref {
                    #bnb::__private::WireLen::Set(_) => ::core::clone::Clone::clone(#by_ref),
                    #bnb::__private::WireLen::Auto => #place.resolve_count(#measure)
                        .map_err(|e| e.in_field(#id_str))?,
                };
                <#ty as #bnb::__private::BitEncode>::bit_encode(&__bnb_resolved, __bnb_w)
                    .map_err(|e| e.in_field(#id_str))?;
            }
        });
    }
    if br.map.is_some() || br.try_map.is_some() {
        return Err(syn::Error::new_spanned(
            f,
            "a `#[br(map = …)]`/`#[br(try_map = …)]` field needs the inverse `#[bw(map = <f>)]` to encode",
        ));
    }
    if br.parse_with.is_some() {
        return Err(syn::Error::new_spanned(
            f,
            "a `#[br(parse_with = …)]` field needs the inverse `#[bw(write_with = <f>)]` to encode",
        ));
    }
    // `if(...)`: a conditional `Option<T>` — write the inner value iff present (the
    // `Option` drives the write; the read-side condition is not re-evaluated).
    if br.cond.is_some() {
        let inner = option_elem(f).ok_or_else(|| {
            syn::Error::new_spanned(f, "`#[br(if(...))]` requires an `Option<_>` field")
        })?;
        let write_inner = quote!(<#inner as #bnb::__private::BitEncode>::bit_encode(__bnb_v, __bnb_w)
            .map_err(|e| e.in_field(#id_str))?;);
        return Ok(quote! {
            if let ::core::option::Option::Some(__bnb_v) = #by_ref {
                #write_inner
            }
        });
    }
    if let Some(elem) = vec_elem(f) {
        let Some(names) = &br.ctx else {
            // Context-free: the element type's `encode_slice` (a byte run is one bulk write),
            // the dual of the `decode_vec` read path.
            return Ok(
                quote!(<#elem as #bnb::__private::BitEncode>::encode_slice(#by_ref, __bnb_w)
                .map_err(|e| e.in_field(#id_str))?;),
            );
        };
        let elem_ctx = ctx_struct_ty(elem)?;
        let lit = site.ctx_literal(&elem_ctx, names);
        Ok(quote! {
            for __bnb_e in #by_ref {
                <#elem as #bnb::EncodeWith<#elem_ctx>>::encode_with(__bnb_e, __bnb_w, #lit)
                    .map_err(|e| e.in_field(#id_str))?;
            }
        })
    } else if let Some(names) = &br.ctx {
        let child_ctx = ctx_struct_ty(ty)?;
        let lit = site.ctx_literal(&child_ctx, names);
        Ok(
            quote!(<#ty as #bnb::EncodeWith<#child_ctx>>::encode_with(#by_ref, __bnb_w, #lit)
            .map_err(|e| e.in_field(#id_str))?;),
        )
    } else if byte_array_len(f).is_some() {
        Ok(quote!(#bnb::__private::write_byte_array(#by_ref, __bnb_w)
            .map_err(|e| e.in_field(#id_str))?;))
    } else {
        // Uniform codec: a leaf or a nested message both encode via `BitEncode`.
        Ok(
            quote!(<#ty as #bnb::__private::BitEncode>::bit_encode(#by_ref, __bnb_w)
            .map_err(|e| e.in_field(#id_str))?;),
        )
    }
}

/// Generates the encode side (`BitEncode` + entry points, or `encode_with` for a
/// `ctx` type). Shared by `#[derive(BitEncode)]` and `#[bin]`. `calc` fields write
/// a computed value; `temp` fields (no `self` field) are written via their `calc`.
/// `decodes` says the caller also generates the decode side for the same fields, so a
/// fixed-length message has a `FixedBitLen` impl its slice encoders can size from.
/// [`field_write_stmt`], but for a field targeted by a struct-level `auto_len(...)` rule:
/// emit a clone-and-fill that resolves each nested `Auto` [`WireLen`](bnb::WireLen) from its
/// sibling before encoding the field. Untargeted fields fall through unchanged. The resolve
/// is mode-independent — an `Auto` has no stored scalar to preserve on the verbatim path.
fn field_write_stmt_auto(
    f: &syn::Field,
    br: &FieldBr,
    site: WriteSite<'_>,
    spec: bool,
    auto_len: &[AutoLenSpec],
) -> syn::Result<TokenStream2> {
    let id = f.ident.as_ref().expect("named field");
    let targeting: Vec<&AutoLenSpec> = auto_len.iter().filter(|s| &s.field == id).collect();
    if targeting.is_empty() {
        return field_write_stmt(f, br, site, spec);
    }
    // An `auto_len` target is written via a clone-and-fill, which can't compose with a
    // codec-overriding directive on the same field (they'd be silently dropped, breaking the
    // encode/decode symmetry). Positioning (`pad_*`/`align_*`) is fine — it wraps the write.
    if br.calc.is_some()
        || br.bw_map.is_some()
        || br.write_with.is_some()
        || br.map.is_some()
        || br.try_map.is_some()
        || br.parse_with.is_some()
        || br.auto_len.is_some()
        || br.cond.is_some()
        || br.count.is_some()
        || br.ctx.is_some()
        || br.temp
        || br.ignore
        || br.restore_position
    {
        return Err(syn::Error::new_spanned(
            f,
            "a field targeted by `#[bin(auto_len(...))]` cannot also carry a codec directive \
             (`calc`/`map`/`write_with`/`parse_with`/`auto_len`/`if`/`count`/`ctx`/`temp`/`ignore`/\
             `restore_position`); only positioning (`pad_*`/`align_*`) may accompany it",
        ));
    }
    let bnb = crate::bnb_path();
    let ty = &f.ty;
    let id_str = crate::printed(id);
    let mut fills = Vec::new();
    for s in targeting {
        let nested = &s.nested;
        let nested_str = crate::printed(nested);
        let source = &s.source;
        let measure = if s.kind == "bytes" {
            bytes_probe(id, source, site, spec)?
        } else {
            quote!(self.#source.len())
        };
        // Only an `Auto` nested value computes the measure (a `bytes` probe isn't free); a
        // `Set(n)` override is kept verbatim.
        fills.push(quote! {
            if #bnb::__private::WireLen::is_auto(&__bnb_auto.#nested) {
                __bnb_auto.#nested = __bnb_auto.#nested.resolve_count(#measure)
                    .map_err(|e| e.in_field(#nested_str))?;
            }
        });
    }
    // Preserve the field's positioning (the decode side applies it too — dropping it here
    // would desync the streams).
    let pre = pad_write_tokens(br.align_before, br.pad_before.as_ref());
    let post = pad_write_tokens(br.align_after, br.pad_after.as_ref());
    Ok(quote! {
        #pre
        {
            let mut __bnb_auto = ::core::clone::Clone::clone(&self.#id);
            #(#fills)*
            <#ty as #bnb::__private::BitEncode>::bit_encode(&__bnb_auto, __bnb_w)
                .map_err(|e| e.in_field(#id_str))?;
        }
        #post
    })
}

/// One field's term, in bits, of the size a variable-length (or encode-only) message's
/// `to_bytes` reserves before it writes: a `Vec`'s length times its element width, a byte
/// array's length, otherwise the type's width. Widths come from `BitLenHint`, which reads
/// zero for a type with no `FixedBitLen`, so the hint requires nothing new of a field.
/// `None` for a field whose size the macro cannot see or that writes nothing.
fn field_bits_hint(f: &syn::Field, br: &FieldBr) -> Option<TokenStream2> {
    if br.ignore || br.br_calc.is_some() || br_indeterminate(br) {
        return None;
    }
    let bnb = crate::bnb_path();
    let width = |ty: &syn::Type| quote!(#bnb::__private::BitLenHint::<#ty>(::core::marker::PhantomData).__bnb_bits());
    if let Some(elem) = vec_elem(f) {
        // A `temp` `Vec` is not on `self`; its length is not known here.
        let id = f.ident.as_ref().filter(|_| !br.temp)?;
        let elem = width(elem);
        Some(quote!(self.#id.len().saturating_mul(#elem)))
    } else if let Some(len) = byte_array_len(f) {
        Some(quote!(usize::saturating_mul(#len, 8)))
    } else {
        Some(width(&f.ty))
    }
}

#[allow(clippy::too_many_lines)] // Verbatim/canonical methods share the same field generation.
pub(in crate::bitstream) fn gen_encode(
    name: &Ident,
    fields: &FieldsNamed,
    attrs: &BitStreamAttrs,
    decodes: bool,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    // Parse each field's directives once (see `gen_decode`), then derive everything
    // from the parsed list.
    let brs: Vec<FieldBr> = fields
        .named
        .iter()
        .map(parse_field_br)
        .collect::<syn::Result<Vec<_>>>()?;
    // Validate each `auto_len(field.nested = …)` names a real, stored field of this struct —
    // a typo would otherwise vanish silently, leaving the nested `WireLen` unresolved (`Auto`)
    // and failing only at encode with an opaque error — and that no two specs target the same
    // `field.nested` path (the second would be a silent no-op, since resolving passes an
    // already-`Set` value through unchanged).
    for (i, spec) in attrs.auto_len.iter().enumerate() {
        let exists = fields
            .named
            .iter()
            .zip(&brs)
            .any(|(f, br)| !br.temp && f.ident.as_ref() == Some(&spec.field));
        if !exists {
            return Err(syn::Error::new_spanned(
                &spec.field,
                format!(
                    "`auto_len` names `{}`, which is not a stored field of this struct",
                    spec.field
                ),
            ));
        }
        if attrs.auto_len[..i]
            .iter()
            .any(|prev| prev.field == spec.field && prev.nested == spec.nested)
        {
            return Err(syn::Error::new_spanned(
                &spec.nested,
                format!(
                    "`auto_len` targets `{}.{}` more than once",
                    spec.field, spec.nested
                ),
            ));
        }
    }
    let indeterminate = !attrs.ctx.is_empty() || brs.iter().any(br_indeterminate);
    let guard = alignment_guard(
        fields,
        attrs.allow_byte_aligned || indeterminate,
        attrs.magic.as_ref(),
    );
    let layout = layout_token(attrs);
    // A fixed-length message hands `to_bytes`/`to_canonical_bytes` its width, so their
    // output buffer is allocated once at its final size.
    let bit_len = (decodes && fixed_len(fields, &brs, attrs))
        .then(|| quote!(<Self as #bnb::__private::FixedBitLen>::BIT_LEN));
    // `magic`: emit the leading constant before the fields (matched read/write).
    let magic_write = match &attrs.magic {
        Some(m) => quote! {
            #bnb::__private::Sink::write(__bnb_w, #m).map_err(|e| e.in_field("magic"))?;
        },
        None => quote!(),
    };
    // Only stored (non-`temp`) fields exist on `self`, for `ctx { … }` resolution.
    let field_set: Vec<&Ident> = fields
        .named
        .iter()
        .zip(&brs)
        .filter(|(_, br)| !br.temp)
        .map(|(f, _)| f.ident.as_ref().expect("named field"))
        .collect();
    let all: Vec<(&syn::Field, &FieldBr)> = fields.named.iter().zip(&brs).collect();
    let site = WriteSite {
        variant: false,
        stored: &field_set,
        fields: &all,
    };
    let writes = fields
        .named
        .iter()
        .zip(&brs)
        .map(|(f, br)| field_write_stmt_auto(f, br, site, false, &attrs.auto_len))
        .collect::<syn::Result<Vec<_>>>()?;

    // `ctx` is **decode-only** by default: if the generated encode body references a ctx
    // param — a `calc`/`bw(map)`/`write_with`/`reserved_with`/positioning expr, or a
    // `ctx { … }` forward passing one down — the type gets `encode_with`/`to_bytes_with`;
    // otherwise a plain `BitEncode`/`to_bytes` (below), so encode needs no context.
    let ctx_names: Vec<&Ident> = attrs.ctx.iter().map(|(n, _)| n).collect();
    let encode_uses_ctx =
        !ctx_names.is_empty() && writes.iter().any(|w| tokens_mention(w.clone(), &ctx_names));
    if encode_uses_ctx {
        let ctx_name = ctx_struct_ident(name);
        let ctx_binds = attrs
            .ctx
            .iter()
            .map(|(n, _)| quote!(let #n = __bnb_ctx.#n;));
        return Ok(quote! {
            #guard
            impl #name {
                #[doc = "Encode to a bit sink, given the context this type declares via `ctx(...)`."]
                #[allow(unused_variables)] // a ctx param may be used on only one side
                pub fn encode_with<__BnbK: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut __BnbK,
                    __bnb_ctx: #ctx_name,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    #(#ctx_binds)*
                    #magic_write
                    #(#writes)*
                    ::core::result::Result::Ok(())
                }
                #[doc = "Encode to a `Vec<u8>` with context."]
                pub fn to_bytes_with(
                    &self,
                    __bnb_ctx: #ctx_name,
                ) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                    #bnb::__private::encode_to_vec_with(#layout, |__bnb_w| self.encode_with(__bnb_w, __bnb_ctx))
                }
            }
            // ctx Layer 2: the polymorphic companion (dual of `DecodeWith`).
            impl #bnb::EncodeWith<#ctx_name> for #name {
                fn encode_with<__BnbK: #bnb::__private::Sink>(
                    &self,
                    __bnb_w: &mut __BnbK,
                    args: #ctx_name,
                ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                    <#name>::encode_with(self, __bnb_w, args)
                }
            }
        });
    }

    // The **canonical** encoder: reserved fields written as their spec value and `calc`
    // fields recomputed (ignoring the stored values). Generated only when it would differ
    // from the verbatim encoder — i.e. the message has a reserved field or a non-`temp`
    // `calc` field (otherwise canonical and verbatim are identical).
    let has_canonical = fields
        .named
        .iter()
        .zip(&brs)
        .any(|(f, br)| field_is_reserved(f) || (br.calc.is_some() && !br.temp));
    let (canonical_method, canonical_inherent) = if has_canonical {
        let writes_canonical = fields
            .named
            .iter()
            .zip(&brs)
            .map(|(f, br)| field_write_stmt_auto(f, br, site, true, &attrs.auto_len))
            .collect::<syn::Result<Vec<_>>>()?;

        // In-memory canonicalization helpers (`to_canonical`/`canonical_diff`/
        // `is_canonical`). For each *stored* (non-`temp`) field the canonical value is: the
        // recomputed `calc` expr, the reserved spec value, or the field itself unchanged.
        let mut calc_precompute = Vec::new();
        let mut field_inits = Vec::new();
        let mut diff_checks = Vec::new();
        for (f, br) in fields.named.iter().zip(&brs) {
            if br.temp {
                continue; // not stored — absent from the struct
            }
            let id = f.ident.as_ref().expect("named field");
            let id_str = crate::printed(id);
            let ty = &f.ty;
            if let Some(calc) = &br.calc {
                // Non-`temp` `calc` (temp filtered above): canonical value = recompute.
                let local = format_ident!("__bnb_canon_{}", id);
                calc_precompute.push(quote!(let #local: #ty = #calc;));
                field_inits.push(quote!(#id: #local));
                diff_checks.push(quote!(if self.#id != (#calc) { __bnb_diff.push(#id_str); }));
            } else if let Some(spec) = reserved_spec_value(f)? {
                // Reserved: canonical value = spec value.
                field_inits.push(quote!(#id: #spec));
                diff_checks.push(quote!(if self.#id != (#spec) { __bnb_diff.push(#id_str); }));
            } else {
                // Ordinary stored field: moved unchanged.
                field_inits.push(quote!(#id: self.#id));
            }
        }

        let encode =
            quote!(|__bnb_w| <Self as #bnb::BitEncode>::canonical_bit_encode(self, __bnb_w));
        let canonical_to_vec = if let Some(bits) = &bit_len {
            quote!(#bnb::__private::encode_fixed_to_vec(#layout, #bits, #encode))
        } else {
            quote!(#bnb::__private::encode_to_vec_with(#layout, #encode))
        };

        // Overrides `BitEncode`'s default (verbatim) `canonical_bit_encode`.
        let method = quote! {
            fn canonical_bit_encode<__BnbK: #bnb::__private::Sink>(
                &self,
                __bnb_w: &mut __BnbK,
            ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                #magic_write
                #(#writes_canonical)*
                ::core::result::Result::Ok(())
            }
        };
        let inherent = quote! {
            impl #name {
                #[doc = "Encode the **canonical** form to a `Vec<u8>`: reserved fields written as their"]
                #[doc = "spec value and `calc` fields recomputed (ignoring the stored values), so the"]
                #[doc = "result is spec-compliant for this message's own fields. (`to_bytes` is verbatim"]
                #[doc = "— it writes exactly what is stored.) Nested messages and `Vec` elements are"]
                #[doc = "written as stored: canonicalize them first if they carry `reserved`/`calc`"]
                #[doc = "fields of their own. To emit this form over a `std::io::Write`, encode the"]
                #[doc = "canonical copy: `value.to_canonical().encode(&mut w)`."]
                pub fn to_canonical_bytes(&self) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                    #canonical_to_vec
                }

                #[doc = "The **canonical form in memory**: a copy with reserved fields set to their"]
                #[doc = "spec value and `calc` fields recomputed. `value.to_canonical().to_bytes()`"]
                #[doc = "equals `value.to_canonical_bytes()`."]
                pub fn to_canonical(self) -> Self {
                    #(#calc_precompute)*
                    Self { #(#field_inits),* }
                }

                #[doc = "The names of the stored fields whose value differs from canonical — i.e."]
                #[doc = "reserved fields not at their spec value, or `calc` fields not equal to their"]
                #[doc = "recomputed value. Empty iff `self` is already canonical."]
                pub fn canonical_diff(&self) -> #bnb::__private::Vec<&'static str> {
                    let mut __bnb_diff = #bnb::__private::Vec::new();
                    #(#diff_checks)*
                    __bnb_diff
                }

                #[doc = "Whether `self` is already in canonical form (none of its own reserved/`calc`"]
                #[doc = "fields differs; nested messages are not inspected)."]
                pub fn is_canonical(&self) -> bool {
                    self.canonical_diff().is_empty()
                }
            }
        };
        (method, inherent)
    } else {
        (quote!(), quote!())
    };

    // A `ctx` type whose encode does *not* read context still impls `EncodeWith` (ignoring
    // the context), so a parent can forward to it uniformly whether or not it needs one.
    let encode_with_trait = if attrs.ctx.is_empty() {
        quote!()
    } else {
        let ctx_name = ctx_struct_ident(name);
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
    };

    let to_vec = if let Some(bits) = &bit_len {
        quote!(#bnb::__private::encode_fixed_to_vec(
            #layout,
            #bits,
            |__bnb_w| <Self as #bnb::BitEncode>::bit_encode(self, __bnb_w),
        ))
    } else {
        // No `FixedBitLen` to size from (a variable-length message, or a lone
        // `#[derive(BitEncode)]`): reserve an estimate summed from the fields.
        let hints = fields
            .named
            .iter()
            .zip(&brs)
            .filter_map(|(f, br)| field_bits_hint(f, br));
        let magic_hint = attrs
            .magic
            .as_ref()
            .map(|m| quote!(.saturating_add(#bnb::__private::bits_of(&#m) as usize)));
        // No lint attribute on the import: a consumer may `forbid` the lint, which an
        // `allow` here would turn into a hard error, and an unused import in a
        // proc-macro's output is not reported.
        quote!({
            use #bnb::__private::NoBitLenHint as _;
            #bnb::__private::encode_hinted_to_vec(
                #layout,
                0usize #(.saturating_add(#hints))* #magic_hint,
                |__bnb_w| <Self as #bnb::BitEncode>::bit_encode(self, __bnb_w),
            )
        })
    };

    Ok(quote! {
        #guard
        impl #bnb::BitEncode for #name {
            const LAYOUT: #bnb::Layout = #layout;
            fn bit_encode<__BnbK: #bnb::__private::Sink>(
                &self,
                __bnb_w: &mut __BnbK,
            ) -> ::core::result::Result<(), #bnb::__private::BitError> {
                #magic_write
                #(#writes)*
                ::core::result::Result::Ok(())
            }
            #canonical_method
        }

        impl #name {
            #[doc = "Encode to a `Vec<u8>`, **verbatim** — exactly what's stored, never silently"]
            #[doc = "rewritten (so `decode` then `to_bytes` round-trips byte-for-byte). For the"]
            #[doc = "spec-normalized form, use `to_canonical_bytes` (generated when the message has"]
            #[doc = "a `reserved` or `calc` field). To write into an explicit bit sink (a `BitWriter`)"]
            #[doc = "or a `std::io::Write`, bring `BitEncode` /"]
            #[doc = "`EncodeExt` into scope and call `.bit_encode(&mut sink)` /"]
            #[doc = "`.encode(&mut w)` (the latter is verbatim, `std` only)."]
            pub fn to_bytes(&self) -> ::core::result::Result<#bnb::__private::Vec<u8>, #bnb::__private::BitError> {
                #to_vec
            }
        }
        #encode_with_trait
        #canonical_inherent
    })
}
