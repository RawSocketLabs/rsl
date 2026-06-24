//! Expansion of the `#[bitflags]` attribute macro: a named set of single-bit
//! flags over one backing integer, with set algebra.
//!
//! Each `bool` field is one flag, auto-assigned a bit by declaration order
//! (LSB-first: the first field is `1 << 0`), or pinned with `#[flag(N)]`.
//! Generates the flag consts, set operators, membership/iteration, per-flag bool
//! accessors, and `Bits`/`Bitfield` impls so a flag set nests in a
//! `#[bitfield]` and serializes.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Ident, ItemStruct, LitInt, Token, Type, parse_macro_input};

struct Args {
    backing: Ident,
    big: bool,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let backing: Ident = input.parse()?;
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
                ("bytes", "be") => big = true,
                ("bytes", "le") => big = false,
                _ => return Err(syn::Error::new_spanned(&key, "expected `bytes = be|le`")),
            }
        }
        Ok(Args { backing, big })
    }
}

struct Flag {
    ident: Ident,
    /// Const name (upper-cased field name): `fin` -> `FIN`.
    konst: Ident,
    /// Bit position.
    bit: u32,
    forward: Vec<Attribute>,
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
    backing_byte_count(backing)?; // validate backing primitive
    let outer: Vec<&Attribute> = item.attrs.iter().collect();

    let flags = collect_flags(&item)?;

    let with_idents: Vec<Ident> = flags
        .iter()
        .map(|f| format_ident!("with_{}", f.ident))
        .collect();
    let set_idents: Vec<Ident> = flags
        .iter()
        .map(|f| format_ident!("set_{}", f.ident))
        .collect();
    let consts: Vec<&Ident> = flags.iter().map(|f| &f.konst).collect();
    let getters: Vec<&Ident> = flags.iter().map(|f| &f.ident).collect();
    let bits: Vec<u32> = flags.iter().map(|f| f.bit).collect();
    let forwards: Vec<&Vec<Attribute>> = flags.iter().map(|f| &f.forward).collect();

    let byte_order_variant = if args.big {
        quote!(Big)
    } else {
        quote!(Little)
    };

    let leaf_codec = crate::bits_leaf_codec_impl(name, &bnb);

    Ok(quote! {
        #(#outer)*
        #vis struct #name {
            value: #backing,
        }

        impl #name {
            #(
                #(#forwards)*
                #[doc = concat!("The `", stringify!(#getters), "` flag (bit ", stringify!(#bits), ").")]
                #vis const #consts: Self = Self { value: (1 as #backing) << #bits };
            )*

            /// The empty set (no flags).
            #vis const fn empty() -> Self { Self { value: 0 } }

            /// All defined flags combined.
            #vis const fn all() -> Self { Self { value: (0 as #backing) #( | ((1 as #backing) << #bits) )* } }

            /// The raw backing integer.
            #vis const fn bits(self) -> #backing { self.value }

            /// From raw bits, **retaining** any unknown (undefined) bits.
            #vis const fn from_bits(value: #backing) -> Self { Self { value } }

            /// From raw bits, dropping any undefined bits.
            #vis const fn from_bits_truncate(value: #backing) -> Self {
                Self { value: value & Self::all().value }
            }

            /// Whether every flag in `other` is set in `self`.
            #vis const fn contains(&self, other: Self) -> bool {
                (self.value & other.value) == other.value
            }

            /// Whether `self` and `other` share any flag.
            #vis const fn intersects(&self, other: Self) -> bool {
                (self.value & other.value) != 0
            }

            /// Whether no flags are set.
            #vis const fn is_empty(&self) -> bool { self.value == 0 }

            /// Adds the flags in `other`.
            #vis fn insert(&mut self, other: Self) { self.value |= other.value; }
            /// Removes the flags in `other`.
            #vis fn remove(&mut self, other: Self) { self.value &= !other.value; }
            /// Flips the flags in `other`.
            #vis fn toggle(&mut self, other: Self) { self.value ^= other.value; }
            /// Inserts or removes `flag` according to `value`.
            #vis fn set(&mut self, flag: Self, value: bool) {
                if value { self.insert(flag) } else { self.remove(flag) }
            }

            /// Const set union (for building combination consts).
            #vis const fn union(self, other: Self) -> Self { Self { value: self.value | other.value } }
            /// Const set intersection.
            #vis const fn intersection(self, other: Self) -> Self { Self { value: self.value & other.value } }
            /// Const set difference (`self` without `other`).
            #vis const fn difference(self, other: Self) -> Self { Self { value: self.value & !other.value } }
            /// Const complement within the defined flags.
            #vis const fn complement(self) -> Self { Self { value: !self.value & Self::all().value } }

            #(
                #[doc = concat!("Whether the `", stringify!(#getters), "` flag is set.")]
                #vis const fn #getters(&self) -> bool { self.contains(Self::#consts) }

                #[doc = concat!("Returns a copy with `", stringify!(#getters), "` set to `value`.")]
                #vis fn #with_idents(mut self, value: bool) -> Self {
                    self.set(Self::#consts, value);
                    self
                }

                #[doc = concat!("Sets `", stringify!(#getters), "` to `value` in place.")]
                #vis fn #set_idents(&mut self, value: bool) { self.set(Self::#consts, value); }
            )*

            /// The list of every defined single-bit flag (used by `iter`).
            const __ALL_FLAGS: &'static [Self] = &[ #( Self::#consts ),* ];

            /// Iterates the single-bit flags that are set, in declaration order.
            #vis fn iter(&self) -> impl Iterator<Item = Self> + '_ {
                let value = self.value;
                Self::__ALL_FLAGS.iter().copied().filter(move |f| (value & f.value) != 0)
            }
        }

        impl ::core::ops::BitOr for #name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self { self.union(rhs) }
        }
        impl ::core::ops::BitAnd for #name {
            type Output = Self;
            fn bitand(self, rhs: Self) -> Self { self.intersection(rhs) }
        }
        impl ::core::ops::BitXor for #name {
            type Output = Self;
            fn bitxor(self, rhs: Self) -> Self { Self { value: self.value ^ rhs.value } }
        }
        impl ::core::ops::Sub for #name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self { self.difference(rhs) }
        }
        impl ::core::ops::Not for #name {
            type Output = Self;
            fn not(self) -> Self { self.complement() }
        }
        impl ::core::ops::BitOrAssign for #name {
            fn bitor_assign(&mut self, rhs: Self) { self.value |= rhs.value; }
        }
        impl ::core::ops::BitAndAssign for #name {
            fn bitand_assign(&mut self, rhs: Self) { self.value &= rhs.value; }
        }
        impl ::core::ops::BitXorAssign for #name {
            fn bitxor_assign(&mut self, rhs: Self) { self.value ^= rhs.value; }
        }
        impl ::core::ops::SubAssign for #name {
            fn sub_assign(&mut self, rhs: Self) { self.value &= !rhs.value; }
        }

        impl #bnb::__private::Bits for #name {
            const BITS: u32 = <#backing>::BITS;
            #[inline]
            fn into_bits(self) -> u128 { self.value as u128 }
            #[inline]
            fn from_bits(raw: u128) -> Self { Self { value: raw as #backing } }
        }

        impl #bnb::__private::Bitfield for #name {
            type Backing = #backing;
            const WIDTH: u32 = <#backing>::BITS;
            const BYTE_ORDER: #bnb::__private::ByteOrder = #bnb::__private::ByteOrder::#byte_order_variant;
            // Flag sets are inherently LSB-indexed (flag n = 1 << n).
            const BIT_ORDER: #bnb::__private::BitOrder = #bnb::__private::BitOrder::Lsb;
            #[inline]
            fn to_raw(self) -> #backing { self.value }
            #[inline]
            fn from_raw(raw: #backing) -> Self { Self { value: raw } }
        }
        #leaf_codec
    })
}

fn collect_flags(item: &ItemStruct) -> syn::Result<Vec<Flag>> {
    let named = match &item.fields {
        syn::Fields::Named(n) => n,
        _ => {
            return Err(syn::Error::new_spanned(
                &item.ident,
                "#[bitflags] requires named `bool` fields",
            ));
        }
    };
    let mut next_bit: u32 = 0;
    let mut flags = Vec::new();
    for f in &named.named {
        let ident = f.ident.clone().expect("named field");
        if !is_bool(&f.ty) {
            return Err(syn::Error::new_spanned(
                &f.ty,
                "each #[bitflags] field must be `bool` (one flag per bit)",
            ));
        }
        let mut explicit: Option<u32> = None;
        let mut forward = Vec::new();
        for attr in &f.attrs {
            if attr.path().is_ident("flag") {
                let lit: LitInt = attr.parse_args()?;
                explicit = Some(lit.base10_parse()?);
            } else {
                forward.push(attr.clone());
            }
        }
        let bit = explicit.unwrap_or(next_bit);
        next_bit = bit + 1;
        let konst = Ident::new(&ident.to_string().to_ascii_uppercase(), ident.span());
        flags.push(Flag {
            ident,
            konst,
            bit,
            forward,
        });
    }
    Ok(flags)
}

fn is_bool(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.is_ident("bool"))
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
