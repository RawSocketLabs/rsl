//! The decode generator: one field's read statement, and `gen_decode` (the `BitDecode` impl
//! or `decode_with`, plus the entry points) over a field list.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{FieldsNamed, Ident};

use super::entry_points::{DECODE_ALL_DOC, DECODE_ITER_DOC, DecodeEntryPoints, PEEK_DOC};
use super::layout::layout_token;
use super::options::BitStreamAttrs;
use super::positioning::pad_read_tokens;
use super::width::{alignment_guard, br_indeterminate, field_width, fixed_len};
use crate::bitstream::ctx::{ctx_literal, ctx_struct_ident, ctx_struct_ty};
use crate::bitstream::field::{FieldBr, byte_array_len, option_elem, parse_field_br, vec_elem};

/// Checked host-size operands shared by generated counts and absolute pointers.
fn checked_usize(expr: &syn::Expr, field: &Ident, message: &str) -> TokenStream2 {
    let bnb = crate::bnb_path();
    let field_str = crate::printed(field);
    quote! {
        ::core::convert::TryInto::<usize>::try_into(#expr).map_err(|_| {
            #bnb::__private::BitError::convert(
                #bnb::__private::String::from(#message),
                #bnb::__private::Source::bit_pos(__bnb_r),
            ).in_field(#field_str)
        })?
    }
}

/// The decode statement for one field — a `let #id = …;` binding, wrapped with any
/// `pad_*`/`align_*` positioning. A later `count` can name an earlier field.
pub(in crate::bitstream) fn field_read_stmt(
    f: &syn::Field,
    br: &FieldBr,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let pre = pad_read_tokens(br.align_before, br.pad_before.as_ref());
    let post = pad_read_tokens(br.align_after, br.pad_after.as_ref());
    // `seek = <bits>`: jump to an absolute bit offset before the read (following a
    // pointer). Read-side only; emitted inside the `restore_position` wrap so the saved
    // offset is the *pre-seek* one (read at the offset, then return).
    let seek = br.seek.as_ref().map(|expr| {
        let id = f.ident.as_ref().expect("named field");
        let offset = checked_usize(expr, id, "seek offset is outside the usize range");
        quote! {
            let __bnb_seek = #offset;
            #bnb::__private::Source::seek_to_bit(__bnb_r, __bnb_seek)?;
        }
    });
    let mut core = field_read_core(f, br)?;
    // `dbg`: trace the field's start offset and decoded value (the field must be
    // `Debug`). Captured after any `seek`, so the offset is where the bits actually came
    // from. TRACE level under target `bnb::dbg` — enable with `RUST_LOG=bnb::dbg=trace`.
    if br.dbg {
        let id = f.ident.as_ref().expect("named field");
        let id_str = crate::printed(id);
        core = quote! {
            let __bnb_dbg_at = #bnb::__private::Source::bit_pos(__bnb_r);
            #core
            #bnb::__private::tracing::trace!(
                target: "bnb::dbg",
                field = #id_str,
                at_bit = __bnb_dbg_at,
                value = ?#id,
            );
        };
    }
    // `assert(...)`: decode-time guards, run in order after the value is bound (and
    // after any `map`, so they see the *mapped* value). Read-only — encode is untouched.
    if !br.asserts.is_empty() {
        let id = f.ident.as_ref().expect("named field");
        let id_str = crate::printed(id);
        let checks = br.asserts.iter().map(|(cond, msg)| {
            let msg_ts = if let Some(args) = msg {
                quote!(#bnb::__private::format!(#args))
            } else {
                quote!(#bnb::__private::String::from(::core::concat!(
                    "assertion failed: `",
                    ::core::stringify!(#cond),
                    "`"
                )))
            };
            quote! {
                if !(#cond) {
                    return ::core::result::Result::Err(
                        #bnb::__private::BitError::convert(
                            #msg_ts,
                            #bnb::__private::Source::bit_pos(__bnb_r),
                        )
                        .in_field(#id_str),
                    );
                }
            }
        });
        core = quote!(#core #(#checks)*);
    }
    let mut body = quote!(#seek #core);
    if br.restore_position {
        // Peek: save the offset (before any seek), read the field, rewind so later
        // fields re-read from where they were.
        body = quote! {
            let __bnb_pos = #bnb::__private::Source::bit_pos(__bnb_r);
            #body
            #bnb::__private::Source::seek_to_bit(__bnb_r, __bnb_pos)?;
        };
    }
    Ok(quote!(#pre #body #post))
}

/// The core decode statement (without positioning).
fn field_read_core(f: &syn::Field, br: &FieldBr) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let id = f.ident.as_ref().expect("named field");
    let id_str = crate::printed(id);
    let ty = &f.ty;
    // A `#[reserved]` field reads as a normal stored leaf here (so the actual wire bits
    // are observable and retained — decode is always verbatim).
    // `ignore`: in-memory only — `Default::default()` on read, no input consumed.
    if br.ignore {
        return Ok(quote!(let #id = ::core::default::Default::default();));
    }
    // `calc`: the read-side dual of `#[bw(calc)]` — bind `expr` (over earlier fields,
    // as locals) with no wire read. Pinned to the declared type so inference can't drift.
    if let Some(calc) = &br.br_calc {
        return Ok(quote!(let #id: #ty = #calc;));
    }
    // `if(<cond>)`: a conditional `Option<T>`. `cond` is over earlier fields (as
    // locals). `Some(read)` when it holds, else `None` (consuming nothing).
    if let Some(cond) = &br.cond {
        let inner = option_elem(f).ok_or_else(|| {
            syn::Error::new_spanned(f, "`#[br(if(...))]` requires an `Option<_>` field")
        })?;
        // Uniform field codec: a leaf (`uN`/bitfield/enum) and a nested message both decode
        // via `BitDecode` (leaves carry the impl now), so no `#[nested]` marker is needed.
        let read_inner = quote!(<#inner as #bnb::__private::BitDecode>::bit_decode(__bnb_r)
            .map_err(|e| e.in_field(#id_str))?);
        return Ok(quote! {
            let #id = if (#cond) {
                ::core::option::Option::Some(#read_inner)
            } else {
                ::core::option::Option::None
            };
        });
    }
    // `map`/`try_map`: read the wire value (`f`'s argument type) and transform it to
    // the field type, pinned to the field's declared type.
    if let Some(map) = &br.map {
        return Ok(
            quote!(let #id: #ty = #bnb::__private::read_mapped(__bnb_r, #map)
            .map_err(|e| e.in_field(#id_str))?;),
        );
    }
    if let Some(try_map) = &br.try_map {
        return Ok(
            quote!(let #id: #ty = #bnb::__private::read_try_mapped(__bnb_r, #try_map)
            .map_err(|e| e.in_field(#id_str))?;),
        );
    }
    // `parse_with`: the escape hatch — a custom `f(r) -> Result<T, BitError>`.
    if let Some(f) = &br.parse_with {
        return Ok(quote!(let #id: #ty = (#f)(__bnb_r)
            .map_err(|e| e.in_field(#id_str))?;));
    }
    if let Some(elem) = vec_elem(f) {
        let count = br.count.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(f, "a `Vec<_>` field needs `#[br(count = <expr>)]`")
        })?;
        // Explicit expressions need the same host-width protection as CountPrefix.
        // Evaluate once; reject negative/oversized counts before touching the payload.
        let checked_count = checked_usize(count, id, "count is outside the usize range");
        let Some(names) = &br.ctx else {
            return Ok(quote! {
                let #id = {
                    let __bnb_n = #checked_count;
                    <#elem as #bnb::__private::BitDecode>::decode_vec(__bnb_r, __bnb_n)
                        .map_err(|e| e.in_field(#id_str))?
                };
            });
        };
        // Read one element into `__bnb_e`, pinning its type so inference can't drift.
        let lit = ctx_literal(&ctx_struct_ty(elem)?, names, None);
        let read_elem = quote! {
            let __bnb_e = <#elem>::decode_with(__bnb_r, #lit)
                .map_err(|e| e.in_field(#id_str))?;
        };
        // No untrusted pre-allocation: `count` is attacker-controlled, so grow the
        // Vec by pushing. Require progress: zero-width/custom codecs need not consume input.
        Ok(quote! {
            let #id = {
                let __bnb_n = #checked_count;
                // One level deeper for the depth budget; the closure returns this loop's errors.
                #bnb::__private::nested_vec(__bnb_r, __bnb_n, |__bnb_r| {
                        let mut __bnb_v: #bnb::__private::Vec<#elem> = #bnb::__private::Vec::new();
                        for _ in 0..__bnb_n {
                            let __bnb_start = #bnb::__private::Source::bit_pos(__bnb_r);
                            #read_elem
                            if #bnb::__private::Source::bit_pos(__bnb_r) <= __bnb_start {
                                return ::core::result::Result::Err(#bnb::__private::BitError::new(
                                    #bnb::__private::ErrorKind::NoProgress, __bnb_start,
                                ).in_field(#id_str));
                            }
                            __bnb_v.push(__bnb_e);
                        }
                        ::core::result::Result::Ok(__bnb_v)
                    })
                    .map_err(|e| e.in_field(#id_str))?
            };
        })
    } else {
        if br.count.is_some() {
            return Err(syn::Error::new_spanned(
                f,
                "`#[br(count = …)]` applies only to a `Vec<_>` field",
            ));
        }
        if let Some(names) = &br.ctx {
            let lit = ctx_literal(&ctx_struct_ty(ty)?, names, None);
            Ok(quote!(let #id = <#ty>::decode_with(__bnb_r, #lit)
                .map_err(|e| e.in_field(#id_str))?;))
        } else if byte_array_len(f).is_some() {
            Ok(quote!(let #id = #bnb::__private::read_byte_array(__bnb_r)
                .map_err(|e| e.in_field(#id_str))?;))
        } else {
            // Uniform codec: a leaf or a nested message both decode via `BitDecode`.
            // (`bit_decode` returns `#ty`, so a `temp` field's type is pinned without an
            // explicit annotation.)
            Ok(
                quote!(let #id = <#ty as #bnb::__private::BitDecode>::bit_decode(__bnb_r)
                .map_err(|e| e.in_field(#id_str))?;),
            )
        }
    }
}

/// Generates the decode side (`BitDecode` + entry points, or `decode_with` for a
/// `ctx` type) from a name + field list + parsed options. Shared by the
/// `#[derive(BitDecode)]` path and by `#[bin]` (which can pass `temp` fields not
/// present in the emitted struct).
#[allow(clippy::too_many_lines)] // Keep generated decode methods and their shared context together.
pub(in crate::bitstream) fn gen_decode(
    name: &Ident,
    fields: &FieldsNamed,
    attrs: &BitStreamAttrs,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    // Parse each field's `#[br]`/`#[bw]` directives once, up front (propagating any
    // parse error immediately), then drive every decision off the parsed list — no
    // re-parsing per predicate.
    let brs: Vec<FieldBr> = fields
        .named
        .iter()
        .map(parse_field_br)
        .collect::<syn::Result<Vec<_>>>()?;
    // A `ctx`/`if`/map/positioning field anywhere makes widths/alignment
    // indeterminable: exempt from the guard and never `FixedBitLen`.
    let indeterminate = !attrs.ctx.is_empty() || brs.iter().any(br_indeterminate);
    let guard = alignment_guard(
        fields,
        attrs.allow_byte_aligned || indeterminate,
        attrs.magic.as_ref(),
    );
    let layout = layout_token(attrs);
    // `magic`: a leading constant read and verified before the fields. Its width
    // (inferred from the value's type) joins `BIT_LEN`.
    let (magic_read, magic_bits) = match &attrs.magic {
        Some(m) => (
            quote! {
                #bnb::__private::verify_magic(__bnb_r, #m).map_err(|e| e.in_field("magic"))?;
            },
            quote!(#bnb::__private::bits_of(&#m) +),
        ),
        None => (quote!(), quote!()),
    };

    // Read each field into a same-named local (declaration order), so a later
    // `count`/`ctx` directive can reference an earlier field; then build `Self`
    // from the **stored** fields only (`#[br(temp)]` reads into a local but is not
    // a struct field).
    let ids: Vec<&Ident> = fields
        .named
        .iter()
        .zip(&brs)
        .filter(|(_, br)| !br.temp)
        .map(|(f, _)| f.ident.as_ref().expect("named field"))
        .collect();
    let read_stmts = fields
        .named
        .iter()
        .zip(&brs)
        .map(|(f, br)| field_read_stmt(f, br))
        .collect::<syn::Result<Vec<_>>>()?;

    // A `ctx(...)`-declaring message takes context it can't get from the plain
    // `BitDecode` trait, so it gets inherent `decode_with`/`decode_with_exact`
    // (binding the ctx params as locals) instead — no `BitDecode`/`FixedBitLen`.
    if !attrs.ctx.is_empty() {
        let ctx_name = ctx_struct_ident(name);
        let ctx_binds = attrs
            .ctx
            .iter()
            .map(|(n, _)| quote!(let #n = __bnb_ctx.#n;));
        return Ok(quote! {
            #guard
            impl #name {
                #[doc = "Decode from a bit source, given the context this type declares via `ctx(...)`."]
                #[allow(unused_variables)] // a ctx param may be used on only one side
                pub fn decode_with<__BnbS: #bnb::__private::Source>(
                    __bnb_r: &mut __BnbS,
                    __bnb_ctx: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #(#ctx_binds)*
                    #magic_read
                    #(#read_stmts)*
                    ::core::result::Result::Ok(Self { #(#ids),* })
                }
                #[doc = "Decode from bytes with context, requiring every whole byte consumed."]
                pub fn decode_with_exact(
                    bytes: &[u8],
                    __bnb_ctx: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_exact_with(bytes, #layout, |__bnb_r| Self::decode_with(__bnb_r, __bnb_ctx))
                }
            }
            // ctx Layer 2: the polymorphic companion, so generic combinators can take
            // this type via `T: DecodeWith<#ctx_name>`.
            impl #bnb::DecodeWith<#ctx_name> for #name {
                fn decode_with<__BnbS: #bnb::__private::Source>(
                    __bnb_r: &mut __BnbS,
                    args: #ctx_name,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    <#name>::decode_with(__bnb_r, args)
                }
            }
        });
    }

    // A message with a `count`-driven `Vec` (or a `ctx`/`if` field) is variable-
    // length; only a fixed one also implements `FixedBitLen` (sizes embedded regions).
    let fixed_bit_len = if fixed_len(fields, &brs, attrs) {
        let widths = fields.named.iter().map(field_width);
        quote! {
            impl #bnb::__private::FixedBitLen for #name {
                const BIT_LEN: u32 = #magic_bits 0 #(+ #widths)*;
            }
        }
    } else {
        quote!()
    };

    // `restore_position` seeks, so the explicit-source entry point requires a
    // [`SeekSource`]; a forward-only stream is then a compile error. Without a seek,
    // any forward `Source` (including a streaming reader) works. The slice entry
    // points (`decode`/`peek`/`decode_exact`) always go through a seekable
    // `BitReader`, so they are unaffected.
    let seeks = brs
        .iter()
        .any(|br| br.restore_position || br.seek.is_some());
    let from_bound = if seeks {
        quote!(#bnb::__private::SeekSource)
    } else {
        quote!(#bnb::__private::Source)
    };
    let from_doc = if seeks {
        "Decode one message from a **seekable** bit cursor (a `BitReader`, `BufSource`, `BitBuf`, \
         …), advancing it. This message uses a seeking directive (`restore_position`/`seek`), so a \
         forward-only stream is rejected at compile time. The byte/bit order is the cursor's — \
         build it with the message's layout, or use `decode_exact`/`decode_all`, which bake it in."
    } else {
        "Decode one message from a bit cursor (a `BitReader`, `BufSource`, `BitBuf`, a streaming \
         reader, …), advancing it. The byte/bit order is the cursor's, not the message's — for a \
         non-default (`little`/`lsb`) order build the cursor with the message's layout (e.g. \
         `BitReader::with_layout`), or use `decode_exact`/`decode_all`, which bake it in."
    };

    // There is no canonical *decode*: `decode_*` is always verbatim (it retains the wire
    // bits of reserved fields — dual-use). Canonicalization is an encode-side concern
    // (`to_canonical_bytes`) or an explicit in-memory helper.
    let entry_points = DecodeEntryPoints {
        name,
        vis: &quote!(pub),
        layout: &layout,
        cursor: &from_bound,
        decode_doc: from_doc,
        decode_all_doc: DECODE_ALL_DOC,
        decode_iter_doc: DECODE_ITER_DOC,
        peek_doc: PEEK_DOC,
        decode_exact_doc: "Decode and require every whole byte consumed (errors with `ErrorKind::TrailingBytes` otherwise).",
    }
    .expand();

    Ok(quote! {
        #guard
        #fixed_bit_len
        impl #bnb::BitDecode for #name {
            fn bit_decode<__BnbS: #bnb::__private::Source>(
                __bnb_r: &mut __BnbS,
            ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                #magic_read
                #(#read_stmts)*
                ::core::result::Result::Ok(Self { #(#ids),* })
            }
        }

        #entry_points
    })
}
