//! One enum variant as a mini-struct: its bindings, its pattern, and its field reads and
//! writes.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Fields, Ident, Type};

use crate::bitstream::field::{D_BW_CALC, FieldBr, field_is_temp, parse_field_br};
use crate::bitstream::generate::{WriteSite, field_read_stmt, field_write_stmt};

/// The bind idents for a variant's fields — the field's own ident (named) or a
/// synthesized `__bnb_f{i}` (tuple). Empty for a unit variant.
pub(super) fn variant_bind_idents(fields: &Fields) -> Vec<Ident> {
    fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            f.ident
                .clone()
                .unwrap_or_else(|| format_ident!("__bnb_f{}", i))
        })
        .collect()
}

/// `Name::Variant { a, b }` / `Name::Variant(a, b)` / `Name::Variant` — serves as
/// **both** the destructuring pattern (encode/`tag`) and the construction expr
/// (decode), which are syntactically identical with field-shorthand.
fn variant_path_fields(
    name: &Ident,
    vid: &Ident,
    fields: &Fields,
    idents: &[Ident],
) -> TokenStream2 {
    match fields {
        Fields::Named(_) => quote!(#name::#vid { #(#idents),* }),
        Fields::Unnamed(_) => quote!(#name::#vid( #(#idents),* )),
        Fields::Unit => quote!(#name::#vid),
    }
}

/// The codec directions an item generates: `read_only` drops `encode`, `write_only` drops
/// `decode`. A dropped direction's statements are not generated at all, so it asks nothing
/// of the fields (no write inverse on a read-only type, no `count` on a write-only one).
#[derive(Clone, Copy)]
pub(super) struct Directions {
    pub(super) decode: bool,
    pub(super) encode: bool,
}

/// Field-level decode/encode for one variant: the per-field reads + the path-with-fields
/// (used as both the decode constructor and the encode pattern) + the per-field writes.
/// For a `#[catch_all]`, `catch_capture` is the discriminant expression bound into the
/// first field on read; that field is omitted from `writes` (the dispatch emits it).
/// `#[br(temp)]` fields are read but dropped from the variant (mirrors a struct). The reads
/// are empty when `dirs.decode` is off, and the writes when `dirs.encode` is.
pub(super) fn variant_field_codec(
    name: &Ident,
    v: &syn::Variant,
    catch_capture: Option<&TokenStream2>,
    named_capture: &[(Ident, Type)],
    dirs: Directions,
) -> syn::Result<(Vec<TokenStream2>, TokenStream2, Vec<TokenStream2>)> {
    let vid = &v.ident;
    let idents = variant_bind_idents(&v.fields);
    let stored_idents: Vec<Ident> = v
        .fields
        .iter()
        .zip(&idents)
        .filter(|(f, _)| !field_is_temp(f))
        .map(|(_, id)| id.clone())
        .collect();
    let path = variant_path_fields(name, vid, &v.fields, &stored_idents);

    let is_catch = catch_capture.is_some();
    if is_catch && v.fields.is_empty() {
        return Err(syn::Error::new_spanned(
            vid,
            "a `#[catch_all]` variant needs a first field to hold the captured discriminant",
        ));
    }

    let mut reads = Vec::new();
    for (i, f) in v.fields.iter().enumerate() {
        let id = &idents[i];
        let br = parse_field_br(f)?;
        if br.calc.is_some() && !br.temp {
            return Err(syn::Error::new(
                br.span_of(D_BW_CALC),
                "`#[bw(calc = …)]` on a stored enum-variant field would never run: a variant \
                 is encoded as stored and an enum has no canonical form. Add `#[br(temp)]` so \
                 the value is computed on every encode instead of stored, or move the \
                 variant's fields into a `#[bin]` struct, whose `to_canonical_bytes` \
                 recomputes a stored `calc`",
            ));
        }
        if let Some((name, ty)) = named_capture
            .iter()
            .find(|(n, _)| f.ident.as_ref() == Some(n))
        {
            let field_ty = &f.ty;
            reads.push(
                quote!(let #id: #field_ty = { let __bnb_captured: #ty = #name; __bnb_captured };),
            );
        } else if is_catch && i == 0 {
            if br.temp {
                return Err(syn::Error::new_spanned(
                    f,
                    "the `#[catch_all]` first field holds the captured discriminant, so it can't be `#[br(temp)]`",
                ));
            }
            let cap = catch_capture.expect("catch capture present");
            reads.push(quote!(let #id = #cap;));
        } else if dirs.decode {
            // (A write-only enum reads nothing, so asks nothing read-side of the field.)
            let mut nf = f.clone();
            nf.ident = Some(id.clone());
            reads.push(field_read_stmt(&nf, &br)?);
        }
    }

    // An enum has no canonical encode, so every variant write is the verbatim pass.
    // Each field is named like its match-arm binding, so the shared generator addresses
    // that local.
    let mut named = Vec::new();
    for (f, id) in v.fields.iter().zip(&idents) {
        let mut nf = f.clone();
        nf.ident = Some(id.clone());
        named.push((nf, parse_field_br(f)?));
    }
    let all: Vec<(&syn::Field, &FieldBr)> = named.iter().map(|(f, br)| (f, br)).collect();
    let stored: Vec<&Ident> = stored_idents.iter().collect();
    let site = WriteSite {
        variant: true,
        stored: &stored,
        fields: &all,
    };
    let mut writes = Vec::new();
    for (i, f) in v.fields.iter().enumerate() {
        if (is_catch && i == 0)
            || named_capture
                .iter()
                .any(|(n, _)| f.ident.as_ref() == Some(n))
        {
            continue; // the captured discriminant — the dispatch emits it
        }
        let (nf, br) = all[i];
        if dirs.encode {
            writes.push(field_write_stmt(nf, br, site, false)?);
        }
    }

    Ok((reads, path, writes))
}
