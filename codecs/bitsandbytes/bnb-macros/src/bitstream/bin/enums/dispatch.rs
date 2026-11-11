//! The parsed and validated dispatch model of a `#[bin]` enum: each variant's role, its
//! tags and magic, and the enum-wide plan.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Fields, Ident, Token, Type};

use super::magic::{Magic, parse_magic};

// ---------------------------------------------------------------------------
// Dispatch model for `#[bin]` enums. Two orthogonal axes:
//   * `tag`   — a read-only selector from `ctx` (never on the wire) that *picks*
//               the variant; takes priority over magic.
//   * `magic` — a wire constant (byte string or byte-aligned unsigned int literal),
//               verified on read and written on encode; the discriminant when there
//               is no tag, or a post-selection signature when there is.
// This module is the parsed+validated model; `bin_enum` (below) is wired onto it.
// ---------------------------------------------------------------------------

/// How a single variant is selected.
#[derive(PartialEq, Eq, Debug)]
pub(super) enum VariantRole {
    /// `#[bin(tag = V)]` — chosen by the selector; no wire signature.
    TagOnly,
    /// `#[bin(tag = V, magic = M)]` — chosen by the selector, then verify `M`.
    TagAndMagic,
    /// `#[bin(magic = M)]` — chosen by matching `M` on the wire.
    MagicOnly,
    /// neither tag nor magic — the typed fallback (at most one).
    Fallback,
    /// `#[catch_all]` — the raw capture (at most one).
    CatchAll,
}

/// One typed equality in a conjunctive variant tag.
pub(super) struct NamedTag {
    /// The `ctx` parameter this condition compares.
    pub(super) name: Ident,
    /// The constant it must equal: an integer literal (range-checked) or a typed expression.
    value: syn::Expr,
}

impl Parse for NamedTag {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![=]>()?;
        Ok(Self {
            name,
            value: input.parse()?,
        })
    }
}

impl NamedTag {
    /// The condition's value as a constant of the parameter type `ty`. An integer literal goes
    /// through `ty`'s `Bits` conversion with a round-trip assert, so `ty` must be a `Bits` type
    /// named directly (`u8`, `u6`, a `BitEnum`); other values are used as written.
    pub(super) fn typed_value(&self, ty: &Type) -> TokenStream2 {
        let value = &self.value;
        if matches!(value, syn::Expr::Lit(lit) if matches!(lit.lit, syn::Lit::Int(_))) {
            let converted = crate::const_from_bits(ty, &quote!(#value as u128));
            let restored = crate::const_into_bits(ty, &quote!(__BNB_VALUE));
            quote!({
                const __BNB_VALUE: #ty = #converted;
                const _: () = assert!(#restored == #value as u128, "tag literal does not fit its context type");
                __BNB_VALUE
            })
        } else {
            // `(7)` is the documented form for a literal of a non-`Bits` type: type-checked,
            // so out of range is an error, and its parentheses are not the user's mistake.
            quote!({
                #[allow(unused_parens, unused_braces)]
                const __BNB_VALUE: #ty = #value;
                __BNB_VALUE
            })
        }
    }
}

/// One variant's dispatch directives.
pub(super) struct VariantDispatch<'a> {
    pub(super) variant: &'a syn::Variant,
    /// `#[bin(tag = V)]` — the selector value to match against.
    pub(super) tag: Option<syn::Expr>,
    /// `#[bin(tag(name = V, ...))]` — conjunctive conditions over named `ctx` parameters.
    pub(super) named_tags: Vec<NamedTag>,
    /// `#[bin(magic = M)]` — the wire signature.
    pub(super) magic: Option<Magic>,
    /// The `magic` literal's span (the call site when there is none), for diagnostics.
    magic_span: proc_macro2::Span,
    pub(super) catch_all: bool,
}

impl VariantDispatch<'_> {
    pub(super) fn role(&self) -> VariantRole {
        if self.catch_all {
            VariantRole::CatchAll
        } else {
            match (
                self.tag.is_some() || !self.named_tags.is_empty(),
                self.magic.is_some(),
            ) {
                (true, true) => VariantRole::TagAndMagic,
                (true, false) => VariantRole::TagOnly,
                (false, true) => VariantRole::MagicOnly,
                (false, false) => VariantRole::Fallback,
            }
        }
    }
}

/// Checks a dispatching (magic-only) variant's constant against the earlier ones. They are
/// all read as one type, so a byte string and an integer cannot be mixed. A repeated constant
/// is accepted: `decode` takes the first declared variant, and the others are reached out of
/// band (`decode_as_<variant>`). A tag variant's magic is a signature checked after
/// selection, so it is not compared.
fn check_dispatch_magic(
    earlier: &[VariantDispatch<'_>],
    vd: &VariantDispatch<'_>,
) -> syn::Result<()> {
    let magic = vd.magic.as_ref().expect("invariant: MagicOnly has a magic");
    let name = &vd.variant.ident;
    for first in earlier
        .iter()
        .filter(|p| p.role() == VariantRole::MagicOnly)
    {
        let prior = first
            .magic
            .as_ref()
            .expect("invariant: MagicOnly has a magic");
        let first = &first.variant.ident;
        if prior.kind() != magic.kind() {
            return Err(syn::Error::new(
                vd.magic_span,
                format!(
                    "variant `{name}`'s `magic` is {}, but variant `{first}`'s is {}: an \
                     enum's dispatching magics are all read as one type. Write every one as \
                     a byte string (`b\"\\x01\"`) or every one as an integer (`0x42u8`)",
                    magic.describe(),
                    prior.describe()
                ),
            ));
        }
    }
    Ok(())
}

/// The uniformity of the variant magic widths — decides single-read vs peek dispatch.
#[derive(PartialEq, Eq, Debug)]
pub(super) enum MagicWidth {
    /// No variant carries a magic.
    None,
    /// Every magic-bearing variant has this same byte width (single-read dispatch).
    Uniform(usize),
    /// Magics differ in width (peek-and-match dispatch — a later step).
    Mixed,
}

/// The parsed + validated dispatch plan for a `#[bin]` enum.
pub(super) struct EnumDispatch<'a> {
    /// `#[bin(tag = <ctx-param>)]` — the selector for tag-variants, if any.
    pub(super) selector: Option<Ident>,
    /// `#[bin(magic = <const>)]` — an optional leading prefix.
    pub(super) prefix: Option<Magic>,
    pub(super) variants: Vec<VariantDispatch<'a>>,
}

impl<'a> EnumDispatch<'a> {
    /// Parses every variant's dispatch directives and validates the structural rules
    /// (a tag-variant needs a declared selector; at most one `#[catch_all]`; at most
    /// one typed fallback). `selector`/`prefix` come from the enum-level `#[bin(...)]`.
    pub(super) fn parse(
        e: &'a syn::ItemEnum,
        selector: Option<Ident>,
        prefix: Option<Magic>,
    ) -> syn::Result<Self> {
        let mut variants = Vec::new();
        let mut catch_alls = 0u32;
        let mut fallbacks = 0u32;
        for v in &e.variants {
            let mut tag = None;
            let mut named_tags = Vec::new();
            let mut magic = None;
            let mut magic_span = proc_macro2::Span::call_site();
            let mut catch_all = false;
            for a in &v.attrs {
                if a.path().is_ident("catch_all") {
                    catch_all = true;
                } else if a.path().is_ident("bin") {
                    a.parse_nested_meta(|m| {
                        if m.path.is_ident("tag") {
                            if !named_tags.is_empty() {
                                return Err(m.error("duplicate tag directive"));
                            }
                            if m.input.peek(syn::token::Paren) {
                                let content;
                                syn::parenthesized!(content in m.input);
                                named_tags =
                                    Punctuated::<NamedTag, Token![,]>::parse_terminated(&content)?
                                        .into_iter()
                                        .collect();
                                if named_tags.is_empty() {
                                    return Err(m.error("a named tag needs at least one condition"));
                                }
                            } else {
                                tag = Some(m.value()?.parse()?);
                            }
                            Ok(())
                        } else if m.path.is_ident("magic") {
                            let expr: syn::Expr = m.value()?.parse()?;
                            magic = Some(parse_magic(&expr)?);
                            magic_span = expr.span();
                            Ok(())
                        } else {
                            Err(m.error(
                                "expected `tag = <value>` or `magic = <literal>` on a variant",
                            ))
                        }
                    })?;
                }
            }
            let vd = VariantDispatch {
                variant: v,
                tag,
                named_tags,
                magic,
                magic_span,
                catch_all,
            };
            match vd.role() {
                VariantRole::CatchAll => catch_alls += 1,
                VariantRole::Fallback => fallbacks += 1,
                VariantRole::TagOnly | VariantRole::TagAndMagic
                    if selector.is_none() && vd.named_tags.is_empty() =>
                {
                    return Err(syn::Error::new_spanned(
                        &v.ident,
                        "a variant with `tag = …` needs the enum to declare the selector via `#[bin(tag = <ctx-param>)]`",
                    ));
                }
                VariantRole::MagicOnly => check_dispatch_magic(&variants, &vd)?,
                _ => {}
            }
            variants.push(vd);
        }
        if catch_alls > 1 {
            return Err(syn::Error::new_spanned(
                &e.ident,
                "a `#[bin]` enum may have at most one `#[catch_all]` variant",
            ));
        }
        if fallbacks > 1 {
            return Err(syn::Error::new_spanned(
                &e.ident,
                "a `#[bin]` enum may have at most one no-tag/no-magic fallback variant",
            ));
        }
        Ok(EnumDispatch {
            selector,
            prefix,
            variants,
        })
    }

    /// Validate the conjunctive form separately so legacy tag/magic dispatch is unchanged.
    pub(super) fn named_selectors(&self, ctx: &[(Ident, Type)]) -> syn::Result<Vec<(Ident, Type)>> {
        let Some(first) = self.variants.iter().find(|v| !v.named_tags.is_empty()) else {
            return Ok(Vec::new());
        };
        let mut selectors = Vec::new();
        for tag in &first.named_tags {
            if selectors.iter().any(|(n, _)| n == &tag.name) {
                return Err(syn::Error::new_spanned(
                    &tag.name,
                    "duplicate named tag condition",
                ));
            }
            let param = ctx.iter().find(|(n, _)| n == &tag.name).ok_or_else(|| {
                syn::Error::new_spanned(&tag.name, "a named tag must name a ctx parameter")
            })?;
            selectors.push(param.clone());
        }
        for v in &self.variants {
            if self.selector.is_some() || v.tag.is_some() || v.magic.is_some() {
                return Err(syn::Error::new_spanned(
                    &v.variant.ident,
                    "named tags cannot be mixed with tag = or variant magic dispatch",
                ));
            }
            if v.catch_all {
                if !v.named_tags.is_empty() {
                    return Err(syn::Error::new_spanned(
                        &v.variant.ident,
                        "a catch-all cannot have tag conditions",
                    ));
                }
                if !matches!(v.variant.fields, Fields::Named(_)) {
                    return Err(syn::Error::new_spanned(
                        &v.variant.ident,
                        "a named-tag catch-all needs named fields for each selector",
                    ));
                }
                for (name, _) in &selectors {
                    let field = v
                        .variant
                        .fields
                        .iter()
                        .find(|f| f.ident.as_ref() == Some(name))
                        .ok_or_else(|| {
                            syn::Error::new_spanned(
                                &v.variant.ident,
                                format!("catch-all is missing selector field `{name}`"),
                            )
                        })?;
                    if field
                        .attrs
                        .iter()
                        .any(|a| ["br", "bw", "brw"].iter().any(|n| a.path().is_ident(n)))
                    {
                        return Err(syn::Error::new_spanned(
                            field,
                            "captured selector fields consume no wire bits and cannot have codec directives",
                        ));
                    }
                }
            } else if !v.named_tags.is_empty()
                && (v.named_tags.len() != selectors.len()
                    || selectors.iter().any(|(n, _)| {
                        v.named_tags.iter().filter(|tag| &tag.name == n).count() != 1
                    }))
            {
                return Err(syn::Error::new_spanned(
                    &v.variant.ident,
                    "every named tag must match the same set of ctx parameters exactly once",
                ));
            }
        }
        Ok(selectors)
    }

    /// The width uniformity of the **dispatching** magics (magic-only variants). A magic
    /// on a tag-variant is a post-selection signature, verified per variant, so it never
    /// participates in the read-once-then-match decision this drives.
    pub(super) fn magic_width(&self) -> MagicWidth {
        let mut width: Option<usize> = None;
        let mut mixed = false;
        for v in &self.variants {
            if v.role() == VariantRole::MagicOnly {
                let len = v.magic.as_ref().expect("MagicOnly has a magic").byte_len();
                match width {
                    None => width = Some(len),
                    Some(__bnb_w) if __bnb_w != len => mixed = true,
                    _ => {}
                }
            }
        }
        match (width, mixed) {
            (_, true) => MagicWidth::Mixed,
            (Some(__bnb_w), false) => MagicWidth::Uniform(__bnb_w),
            (None, false) => MagicWidth::None,
        }
    }

    /// The read type shared by all dispatching magics, if they agree on `(kind, width)`
    /// (a single-read `__bnb_m`); `None` if there are none or they disagree (peek dispatch).
    pub(super) fn uniform_magic_read_type(&self) -> Option<TokenStream2> {
        let mut shape: Option<(u8, usize)> = None;
        let mut ty = None;
        for v in &self.variants {
            if v.role() == VariantRole::MagicOnly {
                let m = v.magic.as_ref().expect("MagicOnly has a magic");
                let s = (m.kind(), m.byte_len());
                match shape {
                    None => {
                        shape = Some(s);
                        ty = Some(m.read_type());
                    }
                    Some(prev) if prev != s => return None,
                    _ => {}
                }
            }
        }
        ty
    }
}

#[cfg(test)]
mod dispatch_tests {
    // Source snippets are kept uniformly as `r#"…"#` (several contain `b"…"` magics).
    #![allow(clippy::needless_raw_string_hashes)]
    use super::{EnumDispatch, Magic, MagicWidth, VariantRole};

    fn enum_of(src: &str) -> syn::ItemEnum {
        syn::parse_str(src).expect("valid enum source")
    }
    fn sel(name: &str) -> syn::Ident {
        syn::parse_str(name).expect("valid ident")
    }

    #[test]
    fn integer_magic_widths_inferred_from_suffix() {
        let src = r#"enum E { #[bin(magic = 0xCAFEu16)] A(u32), #[bin(magic = 0x01u16)] B }"#;
        let e = enum_of(src);
        let d = EnumDispatch::parse(&e, None, None).unwrap();
        assert_eq!(d.variants.len(), 2);
        assert_eq!(d.variants[0].magic.as_ref().unwrap().byte_len(), 2);
        assert_eq!(d.variants[0].role(), VariantRole::MagicOnly);
        assert_eq!(d.magic_width(), MagicWidth::Uniform(2));
    }

    #[test]
    fn byte_string_magic_widths_and_mixed_detection() {
        let e = enum_of(r#"enum E { #[bin(magic = b"IHDR")] A, #[bin(magic = b"END")] B }"#);
        let d = EnumDispatch::parse(&e, None, None).unwrap();
        assert_eq!(d.variants[0].magic.as_ref().unwrap().byte_len(), 4);
        assert_eq!(d.variants[1].magic.as_ref().unwrap().byte_len(), 3);
        assert_eq!(d.magic_width(), MagicWidth::Mixed);
    }

    #[test]
    fn tag_variant_requires_a_selector() {
        let e = enum_of(r#"enum E { #[bin(tag = 1)] A(u8) }"#);
        assert!(EnumDispatch::parse(&e, None, None).is_err());
        let d = EnumDispatch::parse(&e, Some(sel("kind")), None).unwrap();
        assert_eq!(d.variants[0].role(), VariantRole::TagOnly);
    }

    #[test]
    fn tag_and_magic_compose_on_one_variant() {
        let e = enum_of(r#"enum E { #[bin(tag = 1, magic = b"LI")] A(u32) }"#);
        let d = EnumDispatch::parse(&e, Some(sel("kind")), None).unwrap();
        assert_eq!(d.variants[0].role(), VariantRole::TagAndMagic);
    }

    #[test]
    fn fallback_and_catch_all_roles() {
        let e = enum_of(r#"enum E { #[bin(magic = b"X")] A, Plain(u32), #[catch_all] Other(u8) }"#);
        let d = EnumDispatch::parse(&e, None, None).unwrap();
        assert_eq!(d.variants[1].role(), VariantRole::Fallback);
        assert_eq!(d.variants[2].role(), VariantRole::CatchAll);
    }

    #[test]
    fn rejects_invalid_magic_values() {
        let bad = [
            r#"enum E { #[bin(magic = SOME_CONST)] A }"#, // non-literal
            r#"enum E { #[bin(magic = 1)] A }"#,          // unsuffixed int (ambiguous width)
            r#"enum E { #[bin(magic = 1usize)] A }"#,     // non-byte-aligned/platform width
            r#"enum E { #[bin(magic = u4::new(1))] A }"#, // sub-byte, and a non-literal
        ];
        for src in bad {
            assert!(
                EnumDispatch::parse(&enum_of(src), None, None).is_err(),
                "should reject: {src}"
            );
        }
    }

    #[test]
    fn rejects_two_catch_alls_and_two_fallbacks() {
        let two_catch = r#"enum E { #[catch_all] A(u8), #[catch_all] B(u8) }"#;
        assert!(EnumDispatch::parse(&enum_of(two_catch), None, None).is_err());
        let two_fallback = r#"enum E { A(u8), B(u8) }"#;
        assert!(EnumDispatch::parse(&enum_of(two_fallback), None, None).is_err());
    }

    #[test]
    fn prefix_is_recorded() {
        let e = enum_of(r#"enum E { #[bin(magic = b"X")] A }"#);
        let d = EnumDispatch::parse(&e, None, Some(Magic::Bytes(b"PRE".to_vec()))).unwrap();
        assert_eq!(d.prefix.as_ref().unwrap().byte_len(), 3);
    }
}
