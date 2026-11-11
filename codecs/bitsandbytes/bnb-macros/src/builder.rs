//! Shared builder generation for `#[derive(BitsBuilder)]`.
//!
//! The same generator serves two entry points:
//! - the standalone derive on a **plain** struct (fields visible to the derive);
//! - the `#[bitfield]` **intercept** (the attribute macro generates the builder
//!   from the logical fields, *before* it collapses the struct to one integer,
//!   since a real derive could no longer see them).
//!
//! Both produce a `FooBuilder` with `Option`-tracked fields, fluent setters, and
//! a `build()` that returns `Err(BuilderError)` for the first unset **required**
//! field (every field is required unless it carries `#[builder(default)]`).

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::parse_macro_input;
use syn::{Attribute, Data, DeriveInput, Expr, Fields, Ident, Type, Visibility};

/// How an unset field is resolved in `build()`.
// A few short-lived values at macro-expansion time; size is irrelevant.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub(crate) enum FieldDefault {
    /// No `#[builder]`: required — `build()` errors if unset.
    Required,
    /// `#[builder(default)]`: `Default::default()` if unset.
    DefaultTrait,
    /// `#[builder(default = expr)]`: `expr` if unset.
    DefaultExpr(Expr),
}

/// A field the builder exposes.
pub(crate) struct BField {
    pub ident: Ident,
    pub ty: Type,
    pub default: FieldDefault,
}

/// How `build()` constructs the value from the resolved fields.
#[derive(Clone, Copy)]
pub(crate) enum BuildKind {
    /// A `#[bitfield]` type: `Foo::new().with_field(v)…`.
    Bitfield,
    /// A plain struct: `Foo { field: v, … }`.
    Plain,
}

/// Emit dispatch at a concrete field site, not in a generic helper (where Rust
/// would select the opaque fallback before monomorphization).
pub(crate) fn normalize_field(ty: &Type, value: &TokenStream2) -> TokenStream2 {
    let bnb = crate::bnb_path();
    let probe = Ident::new("__bnb_normalize_probe", proc_macro2::Span::mixed_site());
    quote! {
        {
            use #bnb::__private::NormalizeDispatch as _;
            let #probe = #bnb::__private::NormalizeProbe::<#ty>(::core::marker::PhantomData);
            (&#probe).normalize(#value);
        }
    }
}

/// Shared implementation envelope for structs and field-based enums.
pub(crate) fn normalizer_impl(name: &Ident, body: &TokenStream2) -> TokenStream2 {
    let bnb = crate::bnb_path();
    quote! {
        impl #bnb::NormalizeEnumAliases for #name {
            fn normalize_enum_aliases(&mut self) {
                #body
            }
        }
    }
}

/// Parses a `#[builder(...)]` field attribute into a default policy, or `None`
/// if it isn't a builder attribute.
pub(crate) fn parse_builder_attr(attr: &Attribute) -> syn::Result<Option<FieldDefault>> {
    if !attr.path().is_ident("builder") {
        return Ok(None);
    }
    let mut result = FieldDefault::DefaultTrait;
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("default") {
            if meta.input.peek(syn::Token![=]) {
                let expr: Expr = meta.value()?.parse()?;
                result = FieldDefault::DefaultExpr(expr);
            } else {
                result = FieldDefault::DefaultTrait;
            }
            Ok(())
        } else {
            Err(meta.error("expected `default` or `default = <expr>`"))
        }
    })?;
    Ok(Some(result))
}

/// Generates the builder type, `Foo::builder()`, the setters, and `build()`.
///
/// `post_build`, when present, is spliced into `build()` after the value is
/// constructed into the local `__bnb_value` and before it is returned (e.g. to run
/// a soundness validator, `__bnb_value.validate()?;`). It must be a
/// statement (or statements) and may use `?` to short-circuit with a
/// [`bnb::BuilderError`](::bnb::BuilderError).
///
/// # Errors
/// At a field named `build`: its setter would be the builder's own `build()`.
pub(crate) fn generate(
    name: &Ident,
    vis: &Visibility,
    fields: &[BField],
    kind: BuildKind,
    post_build: Option<&TokenStream2>,
) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    // A setter is named after its field, so the only name the builder holds back is its own
    // `build` (two fields cannot share a name, so setters never collide with each other).
    let mut names = crate::GeneratedNames::new(&["build"], "the builder's own `build()`");
    for f in fields {
        names.claim(&f.ident, &f.ident.unraw().to_string(), "its builder setter")?;
    }
    let builder_name = format_ident!("{}Builder", name);
    let idents: Vec<&Ident> = fields.iter().map(|f| &f.ident).collect();
    let tys: Vec<&Type> = fields.iter().map(|f| &f.ty).collect();

    let resolve = fields.iter().map(|f| {
        let id = &f.ident;
        let id_str = crate::printed(id);
        match &f.default {
            FieldDefault::Required => quote! {
                let #id = self.#id.ok_or_else(
                    || #bnb::BuilderError::missing_field(#id_str),
                )?;
            },
            FieldDefault::DefaultTrait => quote!(let #id = self.#id.unwrap_or_default();),
            FieldDefault::DefaultExpr(e) => quote!(let #id = self.#id.unwrap_or_else(|| #e);),
        }
    });

    let construct = match kind {
        BuildKind::Bitfield => {
            let withs = fields.iter().map(|f| {
                let id = &f.ident;
                let with = format_ident!("with_{}", id);
                quote!(.#with(#id))
            });
            quote!(#name::new() #(#withs)*)
        }
        BuildKind::Plain => quote!({
            let mut __bnb_value = #name { #( #idents ),* };
            #bnb::NormalizeEnumAliases::normalize_enum_aliases(&mut __bnb_value);
            __bnb_value
        }),
    };

    Ok(quote! {
        impl #name {
            #[doc = concat!("Returns a builder for [`", stringify!(#name), "`].")]
            #vis fn builder() -> #builder_name {
                // By trait path: a field named `default` has an inherent setter of that name.
                <#builder_name as ::core::default::Default>::default()
            }
        }

        #[doc = concat!(
            "Builder for [`", stringify!(#name),
            "`]; `build` errors on any unset required field."
        )]
        #[derive(Default)]
        #vis struct #builder_name {
            #( #idents: ::core::option::Option<#tys>, )*
        }

        impl #builder_name {
            #(
                #[doc = concat!("Sets `", stringify!(#idents), "`.")]
                #vis fn #idents(mut self, value: #tys) -> Self {
                    self.#idents = ::core::option::Option::Some(value);
                    self
                }
            )*

            /// Resolves fields, normalizes enum aliases, then validates the value.
            /// Returns the first unset required field before normalization or validation.
            #vis fn build(self) -> ::core::result::Result<#name, #bnb::BuilderError> {
                #(#resolve)*
                let __bnb_value = #construct;
                #post_build
                ::core::result::Result::Ok(__bnb_value)
            }
        }
    })
}

/// The standalone `#[derive(BitsBuilder)]` entry, for plain (non-`#[bitfield]`)
/// structs.
pub(crate) fn expand_derive(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match expand_derive_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_derive_inner(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let named = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(n) => n,
            _ => {
                return Err(syn::Error::new_spanned(
                    &input.ident,
                    "BitsBuilder requires a struct with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "BitsBuilder can only derive for structs",
            ));
        }
    };

    let mut fields = Vec::new();
    for f in &named.named {
        let ident = f.ident.clone().expect("named field");
        let ty = f.ty.clone();
        let mut default = FieldDefault::Required;
        for attr in &f.attrs {
            if let Some(d) = parse_builder_attr(attr)? {
                default = d;
            }
        }
        fields.push(BField { ident, ty, default });
    }

    let mut normalize = Vec::new();
    for field in &named.named {
        if crate::bitstream::field_normalizes_aliases(field)? {
            let id = &field.ident;
            normalize.push(normalize_field(&field.ty, &quote!(&mut self.#id)));
        }
    }
    let normalizer = normalizer_impl(&input.ident, &quote!(#(#normalize)*));
    let builder = generate(&input.ident, &input.vis, &fields, BuildKind::Plain, None)?;
    Ok(quote!(#normalizer #builder))
}
