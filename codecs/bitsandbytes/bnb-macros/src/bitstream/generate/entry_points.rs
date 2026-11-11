//! The inherent slice entry points generated beside a codec impl.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// The stateless slice prefix decoders (`decode_prefix`/`decode_prefix_eof`) emitted beside
/// `peek`/`decode_exact` by every context-free decode surface, with `layout` baked in. The
/// runtime reports consumed bits; the generated methods round up to whole bytes.
pub(in crate::bitstream) fn prefix_methods(
    vis: &TokenStream2,
    layout: &TokenStream2,
) -> TokenStream2 {
    let bnb = crate::bnb_path();
    quote! {
        #[doc = "Decode one message from the front of `bytes`, returning it with the number of bytes it consumed. Trailing bytes are ignored; a bit-granular message rounds up, so its final byte's padding counts as consumed."]
        #[doc = ""]
        #[doc = "Stateless: each call replays from the start of `bytes`, and error positions are relative to it. A short slice is `ErrorKind::Incomplete` — retry with the same bytes plus at least `needed` more; don't drain a partial message. Malformed input and logical-region overruns are definitive. See `bnb::guide::io`."]
        #[doc = ""]
        #[doc = "# Errors"]
        #[doc = "`Incomplete` for a short slice, else the definitive decode `BitError` (`NoProgress` if nothing was consumed)."]
        #vis fn decode_prefix(
            bytes: &[u8],
        ) -> ::core::result::Result<(Self, usize), #bnb::__private::BitError> {
            #bnb::__private::decode_prefix(bytes, #layout, false).map(|(v, bits)| (v, bits.div_ceil(8)))
        }
        #[doc = "`decode_prefix` with `bytes` declared to be all the input there is: a short slice is a definitive `ErrorKind::UnexpectedEof` (or `IncompleteAtEof` from a custom codec), never `Incomplete`."]
        #[doc = ""]
        #[doc = "# Errors"]
        #[doc = "The definitive decode `BitError` (`NoProgress` if nothing was consumed)."]
        #vis fn decode_prefix_eof(
            bytes: &[u8],
        ) -> ::core::result::Result<(Self, usize), #bnb::__private::BitError> {
            #bnb::__private::decode_prefix(bytes, #layout, true).map(|(v, bits)| (v, bits.div_ceil(8)))
        }
    }
}
