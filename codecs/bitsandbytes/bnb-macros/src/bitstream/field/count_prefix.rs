//! The `#[brw(count_prefix = <Ty>)]` directive: its scan, and its desugar into the
//! temp + calc + count triad.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Ident, Token};

use super::br::parse_field_br;
use super::shape::vec_elem;

/// One `#[brw(...)]` directive — the bidirectional attr carries only the directives
/// that genuinely act on both sides (`ignore`, `variable`, `count_prefix`).
enum BrwDirective {
    Ignore,
    Variable,
    CountPrefix(Box<syn::Type>),
}

impl Parse for BrwDirective {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let kw: Ident = input.parse()?;
        match kw.to_string().as_str() {
            "ignore" => Ok(BrwDirective::Ignore),
            "variable" => Ok(BrwDirective::Variable),
            "count_prefix" => {
                input.parse::<Token![=]>()?;
                Ok(BrwDirective::CountPrefix(Box::new(input.parse()?)))
            }
            _ => Err(syn::Error::new_spanned(
                kw,
                "unknown `#[brw(...)]` directive; expected `ignore`, `variable`, or `count_prefix = <Ty>`",
            )),
        }
    }
}

/// How the desugared `count_prefix` calc addresses the counted collection: through
/// `self.<id>` (a struct field) or the match-bound local `<id>` (an enum-variant field).
#[derive(Clone, Copy)]
pub(in crate::bitstream) enum CountPrefixSite {
    Struct,
    Variant,
}

/// Scans a field's `#[brw(...)]` attrs for `count_prefix = <Ty>`; removes the directive
/// (keeping a co-listed `ignore`, though the pair is rejected downstream) and returns the
/// prefix type. Duplicates are an error.
pub(in crate::bitstream) fn extract_count_prefix(
    f: &mut syn::Field,
) -> syn::Result<Option<syn::Type>> {
    let mut found: Option<syn::Type> = None;
    let mut rebuilt: Vec<syn::Attribute> = Vec::with_capacity(f.attrs.len());
    for attr in f.attrs.drain(..) {
        if !attr.path().is_ident("brw") {
            rebuilt.push(attr);
            continue;
        }
        let directives =
            attr.parse_args_with(Punctuated::<BrwDirective, Token![,]>::parse_terminated)?;
        if !directives
            .iter()
            .any(|d| matches!(d, BrwDirective::CountPrefix(_)))
        {
            rebuilt.push(attr);
            continue;
        }
        for d in directives {
            match d {
                BrwDirective::Ignore => rebuilt.push(syn::parse_quote!(#[brw(ignore)])),
                BrwDirective::Variable => rebuilt.push(syn::parse_quote!(#[brw(variable)])),
                BrwDirective::CountPrefix(ty) => {
                    if found.is_some() {
                        return Err(syn::Error::new_spanned(&attr, "duplicate `count_prefix`"));
                    }
                    found = Some(*ty);
                }
            }
        }
    }
    f.attrs = rebuilt;
    Ok(found)
}

/// The `#[bin]` `count_prefix` desugar: `#[brw(count_prefix = <Ty>)] items: Vec<T>`
/// expands to the temp+calc+count triad —
///
/// ```text
/// #[br(temp)]
/// #[bw(calc = <checked Ty from items.len()>)]
/// __bnb_count_items: Ty,
/// #[br(count = <items count from __bnb_count_items>)]
/// items: Vec<T>,
/// ```
///
/// — so it rides the existing temp-local decode, calc write, count read, struct-emission
/// temp-drop, and builder/`new` exclusion unchanged. Both conversions are checked through
/// the runtime `CountPrefix` trait: encode never truncates (an oversized `len()` is a
/// `BitError`, not a wrapped prefix), and a `uN` prefix reads/writes its declared width.
pub(in crate::bitstream) fn desugar_count_prefix(
    bnb: &TokenStream2,
    fields: &mut Punctuated<syn::Field, Token![,]>,
    site: CountPrefixSite,
) -> syn::Result<()> {
    let existing: Vec<String> = fields
        .iter()
        .filter_map(|f| f.ident.as_ref().map(ToString::to_string))
        .collect();
    let mut out: Punctuated<syn::Field, Token![,]> = Punctuated::new();
    for mut f in core::mem::take(fields) {
        let Some(pty) = extract_count_prefix(&mut f)? else {
            out.push(f);
            continue;
        };
        if vec_elem(&f).is_none() {
            return Err(syn::Error::new_spanned(
                &f,
                "`#[brw(count_prefix = …)]` applies only to a `Vec<_>` field",
            ));
        }
        // The directive *generates* the length field and the count expression; a
        // directive that reads, stores, or replaces the same machinery conflicts.
        let br = parse_field_br(&f)?;
        if br.count.is_some()
            || br.temp
            || br.calc.is_some()
            || br.br_calc.is_some()
            || br.ignore
            || br.cond.is_some()
            || br.map.is_some()
            || br.try_map.is_some()
            || br.bw_map.is_some()
            || br.parse_with.is_some()
            || br.write_with.is_some()
        {
            return Err(syn::Error::new_spanned(
                &f,
                "`count_prefix` generates the length field and the element count itself; it \
                 can't be combined with `count`, `temp`, `calc`, `ignore`, `if`, \
                 `map`/`try_map`, or `parse_with`/`write_with` on this field",
            ));
        }
        let id = f.ident.clone().expect("checked: named fields only");
        let id_str = crate::printed(&id);
        let count_id = format_ident!("__bnb_count_{}", id, span = id.span());
        let count_name = count_id.to_string();
        if existing.contains(&count_name) {
            return Err(syn::Error::new_spanned(
                &f,
                format!(
                    "`count_prefix` on `{id}` injects a length field named `{count_id}`; \
                     rename that field"
                ),
            ));
        }
        // Encode: the prefix is derived from `len()` — checked, so an oversized
        // collection is a `BitError` pinned to the *user's* field, never a wrapped count.
        let len_expr = match site {
            CountPrefixSite::Struct => quote!(self.#id.len()),
            CountPrefixSite::Variant => quote!(#id.len()),
        };
        let calc_expr = quote! {
            <#pty as #bnb::__private::CountPrefix>::try_from_count(#len_expr)
                .map_err(|__bnb_e| #bnb::__private::BitError::from(__bnb_e)
                    .in_field(#id_str))?
        };
        out.push(syn::Field::parse_named.parse2(quote! {
            #[br(temp)]
            #[bw(calc = #calc_expr)]
            #count_id: #pty
        })?);
        // Decode: a separate `#[br]` attr, so it merges with any user `#[br(ctx {…})]`.
        f.attrs.push(syn::parse_quote!(
            #[br(count = <#pty as #bnb::__private::CountPrefix>::to_count(#count_id))]
        ));
        out.push(f);
    }
    *fields = out;
    Ok(())
}
