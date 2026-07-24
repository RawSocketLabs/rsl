//! Expansion of the `#[bitfield]` attribute macro.
//!
//! The macro cannot know the numeric width of a field whose type is another
//! bitfield/enum (those widths live in `<T as Bits>::BITS`, resolved by the
//! compiler). So instead of computing offsets itself, it emits **const
//! expressions** — `<T as Bits>::BITS`, cumulative sums, and the
//! offset/mask arithmetic — which the compiler evaluates during const-eval. The
//! generated accessors then shift/mask the single backing integer.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Ident, ItemStruct, LitInt, Path, Token, Type, parse_macro_input};

use crate::builder::{self, BField, BuildKind, FieldDefault};

/// Parsed `#[bitfield(backing, bits = …, bytes = …)]` arguments.
struct Args {
    backing: Ident,
    /// `true` = MSB-first (first field in the high bits). Default.
    msb: bool,
    /// `true` = big-endian backing on the wire. Default.
    big: bool,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let backing: Ident = input.parse()?;
        let mut msb = true;
        let mut big = true;
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let val: Ident = input.parse()?;
            match (key.to_string().as_str(), val.to_string().as_str()) {
                ("bits", "msb") => msb = true,
                ("bits", "lsb") => msb = false,
                ("bytes", "big") => big = true,
                ("bytes", "little") => big = false,
                ("bits", other) => {
                    return Err(syn::Error::new_spanned(
                        &val,
                        format!("expected `msb` or `lsb`, got `{other}`"),
                    ));
                }
                ("bytes", other) => {
                    return Err(syn::Error::new_spanned(
                        &val,
                        format!("expected `big` or `little`, got `{other}`"),
                    ));
                }
                (other, _) => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        format!("unknown argument `{other}` (expected `bits` or `bytes`)"),
                    ));
                }
            }
        }
        Ok(Args { backing, msb, big })
    }
}

/// A field's width specification.
enum Spec {
    /// No `#[bits]` — width is `<FieldType as Bits>::BITS`.
    Inferred,
    /// `#[bits(N)]` — explicit width, automatic offset.
    Width(u32),
    /// `#[bits(A..=B)]` — absolute offset and width (manual layout).
    Range(u32, u32),
}

impl Parse for Spec {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let start: LitInt = input.parse()?;
        let start: u32 = start.base10_parse()?;
        if input.peek(Token![..=]) {
            input.parse::<Token![..=]>()?;
            let end: LitInt = input.parse()?;
            let end: u32 = end.base10_parse()?;
            Ok(Spec::Range(start, end))
        } else {
            Ok(Spec::Width(start))
        }
    }
}

struct Field {
    ident: Ident,
    ty: Type,
    spec: Spec,
    /// `#[view(...)]` — a contextual typed view over the field's raw bits (the
    /// stored representation stays the raw bits; the accessor materializes the
    /// typed value). `None` for an ordinary `Bits`-typed field.
    view: Option<View>,
    /// How the builder (if any) treats this field when unset.
    builder_default: FieldDefault,
    /// Doc/cfg attributes to forward onto the generated getter.
    forward: Vec<Attribute>,
}

/// A `#[view(bits = N, read = |raw, s| …, write = |v| …)]` field: the raw `N` bits
/// are stored, and the accessor bridges them to a typed value that may reference
/// sibling fields on read. The `read` closure receives the raw bits and `&Self`
/// (so it can call sibling getters — the context another wire field provides); the
/// `write` closure maps the typed value back to raw bits (context-free). Both bridge
/// through [`Bits`](bnb::Bits), so their raw type is whatever the closures annotate.
struct View {
    bits: u32,
    read: syn::Expr,
    write: syn::Expr,
    /// Explicit `raw = <ty>` — the stored raw type, for the const dispatch. Usually
    /// unnecessary: the type is recovered from the `read` closure's first-parameter
    /// annotation or the `write` closure's return annotation.
    raw: Option<Type>,
    /// `dynamic` — opt out of `const` accessors and call the closures at runtime
    /// (for `read`/`write` bodies that use non-`const` operations).
    dynamic: bool,
    /// `const` — *assert* `const` accessors: a hard error if the raw type isn't
    /// visible (or the `write` body can't be inlined), instead of the quiet
    /// fallback to the runtime closure-call form.
    require_const: bool,
}

impl View {
    /// The stored raw type, if the const dispatch can see one: the explicit
    /// `raw = <ty>`, else the `read` closure's first-parameter annotation, else the
    /// `write` closure's return annotation. `None` means the accessors fall back to
    /// the closure-calling (non-`const`) form, where the type is inferred.
    fn raw_ty(&self) -> Option<Type> {
        if let Some(t) = &self.raw {
            return Some(t.clone());
        }
        if let syn::Expr::Closure(c) = &self.read {
            if let Some(syn::Pat::Type(pt)) = c.inputs.first() {
                return Some((*pt.ty).clone());
            }
        }
        if let syn::Expr::Closure(c) = &self.write {
            if let syn::ReturnType::Type(_, t) = &c.output {
                return Some((**t).clone());
            }
        }
        None
    }
}

/// Whether the token stream contains the `return` keyword at any nesting depth.
/// Inlining a `write` closure body that early-returns would turn "map the value"
/// into "skip the store", so such a body keeps the closure-calling form.
fn contains_return(ts: &TokenStream2) -> bool {
    ts.clone().into_iter().any(|tt| match tt {
        proc_macro2::TokenTree::Ident(i) => i == "return",
        proc_macro2::TokenTree::Group(g) => contains_return(&g.stream()),
        _ => false,
    })
}

/// Parses the arguments of a
/// `#[view(bits = N, read = <closure>, write = <closure>[, raw = <ty>][, const | dynamic])]`.
fn parse_view(attr: &Attribute) -> syn::Result<View> {
    let mut bits: Option<u32> = None;
    let mut read: Option<syn::Expr> = None;
    let mut write: Option<syn::Expr> = None;
    let mut raw: Option<Type> = None;
    let mut dynamic = false;
    let mut require_const = false;
    attr.parse_args_with(|input: ParseStream| {
        while !input.is_empty() {
            // `const` is a keyword, so it can't come out of the `Ident` parse below.
            if input.peek(Token![const]) {
                input.parse::<Token![const]>()?;
                require_const = true;
                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                }
                continue;
            }
            let key: Ident = input.parse()?;
            if key == "dynamic" {
                dynamic = true;
                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                }
                continue;
            }
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "bits" => {
                    let n: LitInt = input.parse()?;
                    bits = Some(n.base10_parse()?);
                }
                "read" => read = Some(input.parse()?),
                "write" => write = Some(input.parse()?),
                "raw" => raw = Some(input.parse()?),
                other => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        format!("unknown `#[view]` argument `{other}` (expected `bits`, `read`, `write`, `raw`, or `dynamic`)"),
                    ));
                }
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(())
    })?;
    let bits = bits.ok_or_else(|| {
        syn::Error::new_spanned(attr, "`#[view(...)]` needs a `bits = <N>` storage width")
    })?;
    let read = read
        .ok_or_else(|| syn::Error::new_spanned(attr, "`#[view(...)]` needs `read = |raw, s| …`"))?;
    let write = write
        .ok_or_else(|| syn::Error::new_spanned(attr, "`#[view(...)]` needs `write = |v| …`"))?;
    if require_const && dynamic {
        return Err(syn::Error::new_spanned(
            attr,
            "`#[view(const)]` and `dynamic` contradict each other: `const` asserts const \
             accessors, `dynamic` opts out of them — keep one",
        ));
    }
    Ok(View {
        bits,
        read,
        write,
        raw,
        dynamic,
        require_const,
    })
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as Args);
    let item = parse_macro_input!(item as ItemStruct);
    match expand_inner(args, item) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_inner(args: Args, item: ItemStruct) -> syn::Result<TokenStream2> {
    let bnb = crate::bnb_path();
    let name = &item.ident;
    let vis = &item.vis;
    let backing = &args.backing;
    let backing_bytes = backing_byte_count(backing)?;

    // Outer attributes minus our own `#[bitfield]` (consumed as `attr`), with the
    // `BitsBuilder` and `Debug` derive markers intercepted (we build the builder, and a
    // logical-field `Debug`, from the fields the std derives can't see post-collapse).
    let (derive_paths, other_attrs, has_builder, has_debug) = split_outer_attrs(&item.attrs)?;
    let derive_attr = if derive_paths.is_empty() {
        quote!()
    } else {
        quote!(#[derive(#(#derive_paths),*)])
    };

    let fields = collect_fields(&item)?;
    let manual = fields.iter().any(|f| matches!(f.spec, Spec::Range(..)));
    if manual && !fields.iter().all(|f| matches!(f.spec, Spec::Range(..))) {
        return Err(syn::Error::new_spanned(
            name,
            "mixing `#[bits(A..=B)]` ranges with inferred/`#[bits(N)]` widths is not allowed; use one style for the whole struct",
        ));
    }
    // Ranges are written low..=high (the offset is the low end). Catch a reversed
    // range with a clear, spanned error instead of a const-eval subtract overflow.
    for f in &fields {
        // Nested rather than a `let`-chain: let-chains are unstable below Rust 1.88,
        // and the MSRV is 1.85.
        if let Spec::Range(a, b) = &f.spec {
            if a > b {
                return Err(syn::Error::new_spanned(
                    &f.ident,
                    format!(
                        "`#[bits({a}..={b})]` is reversed; write the range low..=high (i.e. `{b}..={a}`)"
                    ),
                ));
            }
        }
    }

    // Per-field width const tokens (`<Ty as Bits>::BITS` or a literal).
    let width_ident = |f: &Field| format_ident!("__bits_w_{}", f.ident);
    let off_ident = |f: &Field| format_ident!("__bits_off_{}", f.ident);
    let mask_ident = |f: &Field| format_ident!("__bits_mask_{}", f.ident);

    let bits_path = quote!(#bnb::__private::Bits);

    // 1. Width consts.
    let width_consts = fields.iter().map(|f| {
        let w = width_ident(f);
        let ty = &f.ty;
        let expr = match &f.spec {
            Spec::Inferred => quote!(<#ty as #bits_path>::BITS),
            Spec::Width(n) => quote!(#n),
            Spec::Range(a, b) => {
                let w = b - a + 1;
                quote!(#w)
            }
        };
        quote!(const #w: u32 = #expr;)
    });

    // 2. Total declared width: sum of field widths (auto) or backing width (manual).
    let width_expr = if manual {
        let bits = (backing_bytes * 8) as u32;
        quote!(#bits)
    } else {
        let sum = fields.iter().map(width_ident);
        quote!(0 #( + Self::#sum )*)
    };

    // 3. Offset consts.
    let off_consts: Vec<TokenStream2> = fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let off = off_ident(f);
            let expr = match &f.spec {
                Spec::Range(a, _) => quote!(#a),
                _ if args.msb => {
                    // MSB: offset = WIDTH - (sum of widths up to and including this field).
                    let cum = fields[..=i].iter().map(width_ident);
                    quote!(Self::WIDTH - (0 #( + Self::#cum )*))
                }
                _ => {
                    // LSB: offset = sum of widths before this field.
                    let before = fields[..i].iter().map(width_ident);
                    quote!(0 #( + Self::#before )*)
                }
            };
            quote!(const #off: u32 = #expr;)
        })
        .collect();

    // 4. Mask consts (low `width` bits).
    let mask_consts = fields.iter().map(|f| {
        let m = mask_ident(f);
        let w = width_ident(f);
        quote!(const #m: u128 = if Self::#w >= 128 { u128::MAX } else { (1u128 << Self::#w) - 1 };)
    });

    // 5. Accessors.
    let accessors = fields.iter().map(|f| {
        let ident = &f.ident;
        let ty = &f.ty;
        let off = off_ident(f);
        let mask = mask_ident(f);
        let with = format_ident!("with_{}", ident);
        let set = format_ident!("set_{}", ident);
        let forward = &f.forward;
        // The getter inherits the field's own doc comment (forwarded). If the field
        // is undocumented, emit a fallback so the `pub fn` is never missing-docs in
        // a downstream crate that denies it.
        let getter_doc = if f.forward.iter().any(|a| a.path().is_ident("doc")) {
            quote!()
        } else {
            quote!(#[doc = concat!("Returns the `", stringify!(#ident), "` field.")])
        };
        // A `#[view]` field stores raw bits but presents a typed value: the getter
        // reads the raw bits and hands them (plus `&self`, so it can read sibling
        // fields for context) to the `read` closure; the setter maps the typed value
        // back to raw bits with the context-free `write` closure.
        //
        // When the raw type is visible (`raw = <ty>` or a closure annotation) and the
        // field isn't `dynamic`, the closure bodies are inlined so the accessors can
        // be `const fn` — a `const fn` can neither call a closure nor a trait method,
        // so the conversion also goes through the const dispatch rather than `Bits`.
        // Otherwise the closures are called at runtime and the raw type is inferred
        // from their own annotations — any `Bits` type works with no plumbing here.
        if let Some(view) = &f.view {
            let read = &view.read;
            let write = &view.write;
            let raw_ty = if view.dynamic { None } else { view.raw_ty() };

            // `const` asserts const accessors — turn the quiet runtime fallbacks
            // into spanned errors. (`const` + `dynamic` is rejected at parse time.)
            if view.require_const {
                if raw_ty.is_none() {
                    return syn::Error::new_spanned(
                        ident,
                        "`#[view(const)]` needs the raw type visible to the const dispatch: \
                         annotate the `read` closure's first parameter (`|raw: u2, s| …`), \
                         give the `write` closure a return type, or add `raw = <ty>`",
                    )
                    .to_compile_error();
                }
                if let syn::Expr::Closure(c) = write {
                    let body = &c.body;
                    if c.inputs.len() == 1 && contains_return(&quote!(#body)) {
                        return syn::Error::new_spanned(
                            ident,
                            "`#[view(const)]`: the `write` closure body contains `return`, \
                             which cannot be inlined into a `const` setter; rewrite it \
                             without early `return` (or drop `const`)",
                        )
                        .to_compile_error();
                    }
                }
            }

            let (getter_kw, getter_body) = match &raw_ty {
                Some(rt) => {
                    let from_expr = crate::const_from_bits(rt, &quote!(__masked));
                    let tail = match read {
                        syn::Expr::Closure(c) if c.inputs.len() == 2 => {
                            let p0 = &c.inputs[0];
                            let p1 = &c.inputs[1];
                            let body = &c.body;
                            quote! {
                                let #p0 = #from_expr;
                                let #p1 = self;
                                #body
                            }
                        }
                        // A path to a fn: a direct call is `const`-compatible when
                        // the target is a `const fn`.
                        _ => quote! {
                            let __raw = #from_expr;
                            (#read)(__raw, self)
                        },
                    };
                    (
                        quote!(const fn),
                        quote! {
                            let __masked: u128 = ((self.value as u128) >> Self::#off) & Self::#mask;
                            #tail
                        },
                    )
                }
                None => (
                    quote!(fn),
                    quote! {
                        let __masked: u128 = ((self.value as u128) >> Self::#off) & Self::#mask;
                        let __raw = #bits_path::from_bits(__masked);
                        (#read)(__raw, self)
                    },
                ),
            };

            // The store (shared by `with_*`/`set_*`): typed value -> raw bits -> merge.
            let const_raw_expr = raw_ty.as_ref().and_then(|_| match write {
                syn::Expr::Closure(c) if c.inputs.len() == 1 => {
                    let body = &c.body;
                    if contains_return(&quote!(#body)) {
                        None
                    } else {
                        let p = &c.inputs[0];
                        Some(quote! {{ let #p = value; #body }})
                    }
                }
                _ => Some(quote!((#write)(value))),
            });
            let (store_kw, store) = match (&raw_ty, const_raw_expr) {
                (Some(rt), Some(raw_expr)) => {
                    let into_expr = crate::const_into_bits(rt, &quote!(__raw));
                    (
                        quote!(const fn),
                        quote! {
                            let __raw = #raw_expr;
                            let __bits: u128 = #into_expr;
                            self.value = ((self.value as u128 & !(Self::#mask << Self::#off))
                                | ((__bits & Self::#mask) << Self::#off)) as #backing;
                        },
                    )
                }
                _ => (
                    quote!(fn),
                    quote! {
                        let __raw = (#write)(value);
                        let __bits: u128 = #bits_path::into_bits(__raw);
                        self.value = ((self.value as u128 & !(Self::#mask << Self::#off))
                            | ((__bits & Self::#mask) << Self::#off)) as #backing;
                    },
                ),
            };

            return quote! {
                #getter_doc
                #(#forward)*
                #[inline]
                #vis #getter_kw #ident(&self) -> #ty {
                    #getter_body
                }

                #[doc = concat!("Returns a copy with `", stringify!(#ident), "` set.")]
                #[inline]
                #vis #store_kw #with(mut self, value: #ty) -> Self {
                    #store
                    self
                }

                #[doc = concat!("Sets `", stringify!(#ident), "` in place.")]
                #[inline]
                #vis #store_kw #set(&mut self, value: #ty) {
                    #store
                }
            };
        }
        let from_expr = crate::const_from_bits(ty, &quote!(raw));
        let into_expr = crate::const_into_bits(ty, &quote!(value));
        quote! {
            #getter_doc
            #(#forward)*
            #[inline]
            #vis const fn #ident(&self) -> #ty {
                let raw = ((self.value as u128) >> Self::#off) & Self::#mask;
                #from_expr
            }

            #[doc = concat!("Returns a copy with `", stringify!(#ident), "` set.")]
            #[inline]
            #vis const fn #with(mut self, value: #ty) -> Self {
                let field = (#into_expr & Self::#mask) << Self::#off;
                self.value = ((self.value as u128 & !(Self::#mask << Self::#off)) | field) as #backing;
                self
            }

            #[doc = concat!("Sets `", stringify!(#ident), "` in place.")]
            #[inline]
            #vis const fn #set(&mut self, value: #ty) {
                let field = (#into_expr & Self::#mask) << Self::#off;
                self.value = ((self.value as u128 & !(Self::#mask << Self::#off)) | field) as #backing;
            }
        }
    });

    let bytes_n = backing_bytes;
    let byte_order_variant = if args.big {
        quote!(Big)
    } else {
        quote!(Little)
    };
    let bit_order_variant = if args.msb { quote!(Msb) } else { quote!(Lsb) };

    // The declared byte order (`bytes = big|little`) drives `to_bytes`/`from_bytes`; the
    // endianness-explicit `to_be_bytes`/`to_le_bytes` stay as overrides.
    let (to_decl_bytes, from_decl_bytes, decl_order_lit) = if args.big {
        (quote!(to_be_bytes), quote!(from_be_bytes), "be")
    } else {
        (quote!(to_le_bytes), quote!(from_le_bytes), "le")
    };

    let builder_ts = if has_builder {
        let bfields: Vec<BField> = fields
            .iter()
            .map(|f| BField {
                ident: f.ident.clone(),
                ty: f.ty.clone(),
                default: f.builder_default.clone(),
            })
            .collect();
        builder::generate(name, vis, &bfields, BuildKind::Bitfield, None)
    } else {
        quote!()
    };

    // An intercepted `Debug` derive becomes a custom impl over the *logical* fields (via
    // their getters), so `{:?}` shows `version: 4, ihl: 5` rather than the backing int.
    let debug_ts = if has_debug {
        let field_idents: Vec<_> = fields.iter().map(|f| &f.ident).collect();
        quote! {
            impl ::core::fmt::Debug for #name {
                fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    __f.debug_struct(::core::stringify!(#name))
                        #( .field(::core::stringify!(#field_idents), &self.#field_idents()) )*
                        .finish()
                }
            }
        }
    } else {
        quote!()
    };

    let leaf_codec = crate::bits_leaf_codec_impl(name, &bnb);

    // Layout guarantee: the struct wraps exactly one native integer, so
    // `repr(transparent)` is the honest claim (size/align/ABI of the backing type)
    // — stronger and truer than `repr(C)` would be. A user-supplied `#[repr(...)]`
    // wins (e.g. `repr(C)` for bit-for-bit `bitbybit` parity, or `repr(align(N))`,
    // neither of which can combine with `transparent`).
    let repr_attr = if other_attrs.iter().any(|a| a.path().is_ident("repr")) {
        quote!()
    } else {
        quote!(#[repr(transparent)])
    };

    Ok(quote! {
        #(#other_attrs)*
        #derive_attr
        #repr_attr
        #vis struct #name {
            value: #backing,
        }

        // The layout consts are named after (lowercase) fields, so the
        // upper-case-globals lint is silenced for the generated impl.
        #[allow(non_upper_case_globals)]
        impl #name {
            #(#width_consts)*

            /// The total number of declared bits.
            #vis const WIDTH: u32 = #width_expr;

            #(#off_consts)*
            #(#mask_consts)*

            // Backing-width sanity: declared fields must fit the backing integer.
            const __BITS_FIT: () = assert!(
                Self::WIDTH <= (#bytes_n as u32) * 8,
                "bitfield fields are wider than the backing integer",
            );

            /// Creates an all-zero value.
            #vis const fn new() -> Self {
                Self { value: 0 }
            }

            /// The raw backing integer.
            #vis const fn to_raw(self) -> #backing {
                self.value
            }

            /// Constructs directly from a raw backing integer (no validation).
            #vis const fn from_raw(value: #backing) -> Self {
                Self { value }
            }

            // Const-dispatch seam (see `Bits`): the `Bits::from_bits`/`into_bits`
            // contract as inherent `const fn`s, so this type nests as a field of
            // another `#[bitfield]` whose accessors are `const`. The trait impl
            // delegates here, so the two can't drift.
            #[doc(hidden)]
            #[inline]
            #vis const fn __bnb_from_bits(raw: u128) -> Self {
                Self { value: raw as #backing }
            }

            #[doc(hidden)]
            #[inline]
            #vis const fn __bnb_into_bits(self) -> u128 {
                let m: u128 = if Self::WIDTH >= 128 { u128::MAX } else { (1u128 << Self::WIDTH) - 1 };
                (self.value as u128) & m
            }

            /// The backing integer as big-endian bytes.
            #vis const fn to_be_bytes(self) -> [u8; #bytes_n] {
                self.value.to_be_bytes()
            }

            /// The backing integer as little-endian bytes.
            #vis const fn to_le_bytes(self) -> [u8; #bytes_n] {
                self.value.to_le_bytes()
            }

            /// Constructs from big-endian bytes of the backing integer.
            #vis const fn from_be_bytes(bytes: [u8; #bytes_n]) -> Self {
                Self { value: #backing::from_be_bytes(bytes) }
            }

            /// Constructs from little-endian bytes of the backing integer.
            #vis const fn from_le_bytes(bytes: [u8; #bytes_n]) -> Self {
                Self { value: #backing::from_le_bytes(bytes) }
            }

            #[doc = concat!("The backing integer as bytes in the bitfield's **declared** byte order (`bytes = ", #decl_order_lit, "`).")]
            ///
            /// This is the order-respecting counterpart to the endianness-explicit
            /// [`to_be_bytes`](Self::to_be_bytes)/[`to_le_bytes`](Self::to_le_bytes), which ignore
            /// the declared `bytes =` and always emit the named endianness. Use `to_bytes` to
            /// serialize a standalone bitfield the way it was declared; reach for the explicit
            /// pair only to override that.
            #vis const fn to_bytes(self) -> [u8; #bytes_n] {
                self.#to_decl_bytes()
            }

            #[doc = concat!("Constructs from bytes in the bitfield's **declared** byte order (`bytes = ", #decl_order_lit, "`) — the inverse of [`to_bytes`](Self::to_bytes).")]
            #vis const fn from_bytes(bytes: [u8; #bytes_n]) -> Self {
                Self::#from_decl_bytes(bytes)
            }

            #(#accessors)*
        }

        impl #bits_path for #name {
            const BITS: u32 = Self::WIDTH;
            #[inline]
            fn into_bits(self) -> u128 {
                self.__bnb_into_bits()
            }
            #[inline]
            fn from_bits(raw: u128) -> Self {
                Self::__bnb_from_bits(raw)
            }
        }

        impl #bnb::__private::Bitfield for #name {
            type Backing = #backing;
            const WIDTH: u32 = Self::WIDTH;
            const BYTE_ORDER: #bnb::__private::ByteOrder = #bnb::__private::ByteOrder::#byte_order_variant;
            const BIT_ORDER: #bnb::__private::BitOrder = #bnb::__private::BitOrder::#bit_order_variant;
            #[inline]
            fn to_raw(self) -> #backing { self.value }
            #[inline]
            fn from_raw(raw: #backing) -> Self { Self { value: raw } }
        }

        // Force the width-fit assert. `__BITS_FIT` is an associated const, which Rust
        // only const-evaluates when referenced, so a free `const _` that uses it makes
        // an over-wide bitfield a hard compile error instead of a silent truncation.
        const _: () = #name::__BITS_FIT;

        #leaf_codec
        #builder_ts
        #debug_ts
    })
}

fn collect_fields(item: &ItemStruct) -> syn::Result<Vec<Field>> {
    let named = match &item.fields {
        syn::Fields::Named(n) => n,
        _ => {
            return Err(syn::Error::new_spanned(
                &item.ident,
                "#[bitfield] requires a struct with named fields",
            ));
        }
    };
    named
        .named
        .iter()
        .map(|f| {
            let ident = f.ident.clone().expect("named field");
            let ty = f.ty.clone();
            let mut spec = Spec::Inferred;
            let mut view: Option<View> = None;
            let mut has_bits = false;
            let mut builder_default = FieldDefault::Required;
            let mut forward = Vec::new();
            for attr in &f.attrs {
                if attr.path().is_ident("bits") {
                    spec = attr.parse_args::<Spec>()?;
                    has_bits = true;
                } else if attr.path().is_ident("view") {
                    view = Some(parse_view(attr)?);
                } else if let Some(d) = builder::parse_builder_attr(attr)? {
                    builder_default = d;
                } else {
                    forward.push(attr.clone());
                }
            }
            // A view carries its own storage width, so it can't also take `#[bits]`;
            // its width becomes an ordinary auto-placed `Width` for the layout machinery.
            if let Some(v) = &view {
                if has_bits {
                    return Err(syn::Error::new_spanned(
                        &ident,
                        "a `#[view(...)]` field's width comes from its `bits = <N>`; drop the separate `#[bits(...)]`",
                    ));
                }
                spec = Spec::Width(v.bits);
            }
            Ok(Field {
                ident,
                ty,
                spec,
                view,
                builder_default,
                forward,
            })
        })
        .collect()
}

/// Splits the struct's outer attributes into the kept `#[derive(...)]` paths and the
/// other attributes, intercepting two derives: `BitsBuilder` (so `#[bitfield]` can build
/// the builder from the fields the derive can't see after the struct is collapsed) and
/// `Debug` (so `#[bitfield]` can emit a custom `Debug` that decomposes the *logical*
/// fields, e.g. `version=4, ihl=5`, instead of the std derive's opaque `{ value: 69 }`).
/// Reports whether each marker was present.
fn split_outer_attrs(attrs: &[Attribute]) -> syn::Result<(Vec<Path>, Vec<Attribute>, bool, bool)> {
    let mut derive_paths = Vec::new();
    let mut others = Vec::new();
    let mut has_builder = false;
    let mut has_debug = false;
    for attr in attrs {
        if attr.path().is_ident("derive") {
            let paths = attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)?;
            for p in paths {
                if p.is_ident("BitsBuilder") {
                    has_builder = true;
                } else if p.is_ident("Debug") {
                    has_debug = true;
                } else {
                    derive_paths.push(p);
                }
            }
        } else {
            others.push(attr.clone());
        }
    }
    Ok((derive_paths, others, has_builder, has_debug))
}

fn backing_byte_count(backing: &Ident) -> syn::Result<usize> {
    match backing.to_string().as_str() {
        "u8" => Ok(1),
        "u16" => Ok(2),
        "u32" => Ok(4),
        "u64" => Ok(8),
        "u128" => Ok(16),
        other => Err(syn::Error::new_spanned(
            backing,
            format!("backing must be u8/u16/u32/u64/u128, got `{other}`"),
        )),
    }
}
