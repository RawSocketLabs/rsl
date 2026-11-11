//! `Magic`: a `magic` wire constant of a `#[bin]` enum, with its read, verify and write.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Ident;

/// A `magic` wire constant. Restricted to **byte-oriented literals** so its on-wire
/// width is unambiguous: a byte string (`b"IHDR"`) or a width-suffixed unsigned
/// integer (`0x01u16`). Sub-byte types (`u4`) and non-literals are rejected.
pub(super) enum Magic {
    /// A byte-string/byte literal — its raw bytes.
    Bytes(Vec<u8>),
    /// A byte-aligned unsigned integer literal — the expression and its byte width.
    /// `value` is boxed: `syn::Expr` is large (~240 bytes), so an unboxed variant would
    /// dominate the enum's size (`clippy::large_enum_variant`).
    Int { value: Box<syn::Expr>, width: usize },
}

/// The unsigned integer type token for a byte width (1/2/4/8/16).
fn int_type_for_width(width: usize) -> TokenStream2 {
    match width {
        1 => quote!(u8),
        2 => quote!(u16),
        4 => quote!(u32),
        8 => quote!(u64),
        16 => quote!(u128),
        _ => unreachable!("magic int width is validated to 1/2/4/8/16"),
    }
}

impl Magic {
    /// The on-wire byte length of this magic.
    pub(super) fn byte_len(&self) -> usize {
        match self {
            Magic::Bytes(b) => b.len(),
            Magic::Int { width, .. } => *width,
        }
    }

    /// A coarse shape discriminator (byte-string vs integer) for "all magics in this
    /// enum read the same way" checks — two magics dispatch uniformly iff their
    /// `(kind, byte_len)` agree.
    pub(super) fn kind(&self) -> u8 {
        match self {
            Magic::Bytes(_) => 0,
            Magic::Int { .. } => 1,
        }
    }

    /// The magic's kind, for a diagnostic ("… is a byte string").
    pub(super) fn describe(&self) -> &'static str {
        match self {
            Magic::Bytes(_) => "a byte string",
            Magic::Int { .. } => "an integer",
        }
    }

    /// The type this magic is read into: `[u8; N]` for a byte string, the unsigned
    /// integer type for an int.
    pub(super) fn read_type(&self) -> TokenStream2 {
        match self {
            Magic::Bytes(b) => {
                let n = b.len();
                quote!([u8; #n])
            }
            Magic::Int { width, .. } => int_type_for_width(*width),
        }
    }

    /// A `read_type`-valued constant equal to this magic — for `==` dispatch and for
    /// writing a known variant's magic.
    pub(super) fn const_expr(&self) -> TokenStream2 {
        match self {
            Magic::Bytes(b) => {
                let bytes = b.iter();
                quote!([#(#bytes),*])
            }
            Magic::Int { value, .. } => quote!(#value),
        }
    }

    /// Read this magic from `r` into the local `binding`.
    pub(super) fn read_into(&self, binding: &Ident) -> TokenStream2 {
        let bnb = crate::bnb_path();
        let ty = self.read_type();
        match self {
            Magic::Bytes(_) => quote!(
                let #binding: #ty = #bnb::__private::read_byte_array(__bnb_r).map_err(|e| e.in_field("magic"))?;
            ),
            Magic::Int { .. } => quote!(
                let #binding: #ty = #bnb::__private::Source::read(__bnb_r).map_err(|e| e.in_field("magic"))?;
            ),
        }
    }

    /// Read and verify this magic, erroring on mismatch (`what` names the site).
    pub(super) fn verify(&self, what: &str) -> TokenStream2 {
        let bnb = crate::bnb_path();
        let binding = format_ident!("__bnb_vm");
        let read = self.read_into(&binding);
        let expected = self.const_expr();
        let msg = format!("magic mismatch ({what})");
        quote! {
            #read
            if #binding != #expected {
                return ::core::result::Result::Err(
                    #bnb::__private::BitError::convert(
                        #bnb::__private::String::from(#msg),
                        #bnb::__private::Source::bit_pos(__bnb_r),
                    ).in_field("magic"),
                );
            }
        }
    }

    /// Write `value` (a `read_type`-typed expression) as this magic to `w`.
    fn write_value(&self, value: &TokenStream2) -> TokenStream2 {
        let bnb = crate::bnb_path();
        match self {
            Magic::Bytes(_) => quote!(
                #bnb::__private::write_byte_array(&#value, __bnb_w).map_err(|e| e.in_field("magic"))?;
            ),
            Magic::Int { .. } => quote!(
                #bnb::__private::Sink::write(__bnb_w, #value).map_err(|e| e.in_field("magic"))?;
            ),
        }
    }

    /// Write this magic's constant value to `w` (for a known variant).
    pub(super) fn write_const(&self) -> TokenStream2 {
        let c = self.const_expr();
        self.write_value(&c)
    }
}

/// Parses + validates a `magic = <literal>` value into a [`Magic`].
pub(super) fn parse_magic(expr: &syn::Expr) -> syn::Result<Magic> {
    if let syn::Expr::Lit(syn::ExprLit { lit, .. }) = expr {
        match lit {
            syn::Lit::ByteStr(s) => return Ok(Magic::Bytes(s.value())),
            syn::Lit::Byte(b) => return Ok(Magic::Bytes(vec![b.value()])),
            syn::Lit::Int(li) => {
                let width = match li.suffix() {
                    "u8" => 1usize,
                    "u16" => 2,
                    "u32" => 4,
                    "u64" => 8,
                    "u128" => 16,
                    "" => {
                        return Err(syn::Error::new_spanned(
                            expr,
                            "a `magic` integer needs a width suffix so its wire size is unambiguous, e.g. `0x01u16`",
                        ));
                    }
                    other => {
                        return Err(syn::Error::new_spanned(
                            expr,
                            format!(
                                "`{other}` is not a valid `magic` type; use a byte-aligned unsigned integer (u8/u16/u32/u64/u128) or a byte string"
                            ),
                        ));
                    }
                };
                return Ok(Magic::Int {
                    value: Box::new(expr.clone()),
                    width,
                });
            }
            _ => {}
        }
    }
    Err(syn::Error::new_spanned(
        expr,
        "a `magic` must be a byte string (`b\"…\"`) or a byte-aligned unsigned integer literal (`0x01u16`)",
    ))
}
