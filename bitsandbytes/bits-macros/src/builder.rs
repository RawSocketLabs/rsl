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
pub(crate) enum BuildKind {
    /// A `#[bitfield]` type: `Foo::new().with_field(v)…`.
    Bitfield,
    /// A plain struct: `Foo { field: v, … }`.
    Plain,
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
/// constructed into the local `__value` and before it is returned — `#[wire]`
/// uses it to run the soundness validator (`__value.validate()?;`). It must be a
/// statement (or statements) and may use `?` to short-circuit with a
/// [`bits::BuilderError`](::bits::BuilderError).
pub(crate) fn generate(
    name: &Ident,
    vis: &Visibility,
    fields: &[BField],
    kind: BuildKind,
    post_build: Option<&TokenStream2>,
) -> TokenStream2 {
    let builder_name = format_ident!("{}Builder", name);
    let idents: Vec<&Ident> = fields.iter().map(|f| &f.ident).collect();
    let tys: Vec<&Type> = fields.iter().map(|f| &f.ty).collect();

    let resolve = fields.iter().map(|f| {
        let id = &f.ident;
        match &f.default {
            FieldDefault::Required => quote! {
                let #id = self.#id.ok_or_else(
                    || ::bits::BuilderError::missing_field(stringify!(#id)),
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
        BuildKind::Plain => quote!(#name { #( #idents ),* }),
    };

    quote! {
        impl #name {
            #[doc = concat!("Returns a builder for [`", stringify!(#name), "`].")]
            #vis fn builder() -> #builder_name {
                #builder_name::default()
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

            /// Builds the value, or returns the first unset required field.
            #vis fn build(self) -> ::core::result::Result<#name, ::bits::BuilderError> {
                #(#resolve)*
                let __value = #construct;
                #post_build
                ::core::result::Result::Ok(__value)
            }
        }
    }
}

/// The standalone `#[derive(BitsBuilder)]` entry, for plain (non-`#[bitfield]`)
/// structs.
pub fn expand_derive(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match expand_derive_inner(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_derive_inner(input: DeriveInput) -> syn::Result<TokenStream2> {
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

    Ok(generate(
        &input.ident,
        &input.vis,
        &fields,
        BuildKind::Plain,
        None,
    ))
}
