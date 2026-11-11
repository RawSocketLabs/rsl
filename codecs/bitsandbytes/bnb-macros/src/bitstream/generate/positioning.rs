//! Positioning statements (`pad_*` / `align_*`) emitted around a field's read or write.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// Positioning statements emitted before/after a field: `align_*` skips to the next
/// byte boundary, `pad_*` skips a bit count.
pub(super) fn pad_read_tokens(align: bool, pad: Option<&syn::Expr>) -> TokenStream2 {
    let bnb = crate::bnb_path();
    let align = align.then(|| quote!(#bnb::__private::align_read(__bnb_r)?;));
    let pad = pad.map(|n| quote!(#bnb::__private::skip_read(__bnb_r, #n)?;));
    quote!(#align #pad)
}

pub(super) fn pad_write_tokens(align: bool, pad: Option<&syn::Expr>) -> TokenStream2 {
    let bnb = crate::bnb_path();
    let align = align.then(|| quote!(#bnb::__private::align_write(__bnb_w)?;));
    let pad = pad.map(|n| quote!(#bnb::__private::skip_write(__bnb_w, #n)?;));
    quote!(#align #pad)
}
