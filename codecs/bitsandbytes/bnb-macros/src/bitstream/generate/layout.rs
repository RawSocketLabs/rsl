//! A message's runtime `Layout` (bit + byte order): the token for it, and the hidden
//! `DecodeLayout` impl of a decode-only type.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Ident;

use super::options::BitStreamAttrs;

/// The hidden `DecodeLayout` impl a decode-only (`read_only`) `#[bin]` type carries: its
/// layout, which otherwise lives on the `BitEncode` it does not implement, so the type can
/// still be the wire type of a decode-only mapped struct.
pub(in crate::bitstream) fn decode_layout_impl(
    name: &Ident,
    layout: &TokenStream2,
) -> TokenStream2 {
    let bnb = crate::bnb_path();
    quote! {
        impl #bnb::__private::DecodeLayout for #name {
            fn layout() -> #bnb::__private::Layout {
                #layout
            }
        }
    }
}

/// The runtime [`Layout`](bnb::Layout) (bit + byte order) for the struct.
pub(in crate::bitstream) fn layout_token(attrs: &BitStreamAttrs) -> TokenStream2 {
    let bnb = crate::bnb_path();
    let bit = if attrs.lsb {
        quote!(#bnb::__private::BitOrder::Lsb)
    } else {
        quote!(#bnb::__private::BitOrder::Msb)
    };
    let byte = if attrs.little {
        quote!(#bnb::__private::ByteOrder::Little)
    } else {
        quote!(#bnb::__private::ByteOrder::Big)
    };
    quote!(#bnb::__private::Layout { bit: #bit, byte: #byte })
}
