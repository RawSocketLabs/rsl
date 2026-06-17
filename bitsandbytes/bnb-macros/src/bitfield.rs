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
                ("bytes", "be") => big = true,
                ("bytes", "le") => big = false,
                ("bits", other) => {
                    return Err(syn::Error::new_spanned(
                        &val,
                        format!("expected `msb` or `lsb`, got `{other}`"),
                    ));
                }
                ("bytes", other) => {
                    return Err(syn::Error::new_spanned(
                        &val,
                        format!("expected `be` or `le`, got `{other}`"),
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
    /// How the builder (if any) treats this field when unset.
    builder_default: FieldDefault,
    /// Doc/cfg attributes to forward onto the generated getter.
    forward: Vec<Attribute>,
}

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as Args);
    let item = parse_macro_input!(item as ItemStruct);
    match expand_inner(args, item) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_inner(args: Args, item: ItemStruct) -> syn::Result<TokenStream2> {
    let name = &item.ident;
    let vis = &item.vis;
    let backing = &args.backing;
    let backing_bytes = backing_byte_count(backing)?;

    // Outer attributes minus our own `#[bitfield]` (consumed as `attr`), with the
    // `BitsBuilder` derive marker intercepted so we can build it from the fields.
    let (derive_paths, other_attrs, has_builder) = split_outer_attrs(&item.attrs)?;
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
        if let Spec::Range(a, b) = &f.spec
            && a > b
        {
            return Err(syn::Error::new_spanned(
                &f.ident,
                format!(
                    "`#[bits({a}..={b})]` is reversed; write the range low..=high (i.e. `{b}..={a}`)"
                ),
            ));
        }
    }

    // Per-field width const tokens (`<Ty as Bits>::BITS` or a literal).
    let width_ident = |f: &Field| format_ident!("__bits_w_{}", f.ident);
    let off_ident = |f: &Field| format_ident!("__bits_off_{}", f.ident);
    let mask_ident = |f: &Field| format_ident!("__bits_mask_{}", f.ident);

    let bits_path = quote!(::bnb::__private::Bits);

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
        quote! {
            #getter_doc
            #(#forward)*
            #[inline]
            #vis fn #ident(&self) -> #ty {
                let raw = ((self.value as u128) >> Self::#off) & Self::#mask;
                <#ty as #bits_path>::from_bits(raw)
            }

            #[doc = concat!("Returns a copy with `", stringify!(#ident), "` set.")]
            #[inline]
            #vis fn #with(mut self, value: #ty) -> Self {
                let field = (<#ty as #bits_path>::into_bits(value) & Self::#mask) << Self::#off;
                self.value = ((self.value as u128 & !(Self::#mask << Self::#off)) | field) as #backing;
                self
            }

            #[doc = concat!("Sets `", stringify!(#ident), "` in place.")]
            #[inline]
            #vis fn #set(&mut self, value: #ty) {
                let field = (<#ty as #bits_path>::into_bits(value) & Self::#mask) << Self::#off;
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

    Ok(quote! {
        #(#other_attrs)*
        #derive_attr
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
            #vis const fn raw(self) -> #backing {
                self.value
            }

            /// Constructs directly from a raw backing integer (no validation).
            #vis const fn from_raw(value: #backing) -> Self {
                Self { value }
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

            #(#accessors)*
        }

        impl #bits_path for #name {
            const BITS: u32 = Self::WIDTH;
            #[inline]
            fn into_bits(self) -> u128 {
                let m: u128 = if Self::WIDTH >= 128 { u128::MAX } else { (1u128 << Self::WIDTH) - 1 };
                (self.value as u128) & m
            }
            #[inline]
            fn from_bits(raw: u128) -> Self {
                Self { value: raw as #backing }
            }
        }

        impl ::bnb::__private::Bitfield for #name {
            type Backing = #backing;
            const WIDTH: u32 = Self::WIDTH;
            const BYTE_ORDER: ::bnb::__private::ByteOrder = ::bnb::__private::ByteOrder::#byte_order_variant;
            const BIT_ORDER: ::bnb::__private::BitOrder = ::bnb::__private::BitOrder::#bit_order_variant;
            #[inline]
            fn to_raw(self) -> #backing { self.value }
            #[inline]
            fn from_raw(raw: #backing) -> Self { Self { value: raw } }
        }
        #builder_ts
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
            let mut builder_default = FieldDefault::Required;
            let mut forward = Vec::new();
            for attr in &f.attrs {
                if attr.path().is_ident("bits") {
                    spec = attr.parse_args::<Spec>()?;
                } else if let Some(d) = builder::parse_builder_attr(attr)? {
                    builder_default = d;
                } else {
                    forward.push(attr.clone());
                }
            }
            Ok(Field {
                ident,
                ty,
                spec,
                builder_default,
                forward,
            })
        })
        .collect()
}

/// Splits the struct's outer attributes into the kept `#[derive(...)]` paths
/// (with `BitsBuilder` removed) and the other attributes, reporting whether the
/// `BitsBuilder` marker was present so `#[bitfield]` can generate the builder
/// itself (the derive can't see the fields after the struct is collapsed).
fn split_outer_attrs(attrs: &[Attribute]) -> syn::Result<(Vec<Path>, Vec<Attribute>, bool)> {
    let mut derive_paths = Vec::new();
    let mut others = Vec::new();
    let mut has_builder = false;
    for attr in attrs {
        if attr.path().is_ident("derive") {
            let paths = attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)?;
            for p in paths {
                if p.is_ident("BitsBuilder") {
                    has_builder = true;
                } else {
                    derive_paths.push(p);
                }
            }
        } else {
            others.push(attr.clone());
        }
    }
    Ok((derive_paths, others, has_builder))
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
