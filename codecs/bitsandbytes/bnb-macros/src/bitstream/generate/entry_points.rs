//! The inherent decode entry points generated beside a `BitDecode` impl: one generator,
//! used by every front-end.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Ident;

/// `decode_all`'s doc on a field-coded message (a struct's or an enum's own fields).
pub(in crate::bitstream) const DECODE_ALL_DOC: &str = "Decode every message from `bytes` into a `Vec`, bit-aware with the message's own byte/bit order baked in. Whole bytes must hold whole messages; a sub-byte tail in the final byte that does not decode is padding, as in `decode_exact`. A message that decodes without ending past its start (zero-width, or a backward `seek`) is a `NoProgress` error.";

/// `decode_iter`'s doc on a field-coded message.
pub(in crate::bitstream) const DECODE_ITER_DOC: &str = "A lazy iterator decoding successive messages from `bytes` (layout baked in) until it is drained, ending after the first error if one occurs. A sub-byte tail in the final byte that does not decode ends it cleanly, as padding. A message that decodes without ending past its start (zero-width, or a backward `seek`) ends it with a `NoProgress` error.";

/// `peek`'s doc on a field-coded message.
pub(in crate::bitstream) const PEEK_DOC: &str = "Decode one message from `bytes` without consuming the caller's buffer (tail-tolerant). Not a prefix decoder: a short slice is a definitive `UnexpectedEof`, indistinguishable from malformed input — use `decode_prefix` to tell them apart.";

/// The inherent decoders of a context-free type, as one `impl` block: `decode` over a bit
/// cursor, then the slice entry points with the layout baked in (`decode_all`, `decode_iter`,
/// `peek`, `decode_exact`, `decode_prefix`, `decode_prefix_eof`). Every front-end emits this
/// block through [`expand`](Self::expand); the fields are all that differs between them.
pub(in crate::bitstream) struct DecodeEntryPoints<'a> {
    /// The type the methods are generated on.
    pub(in crate::bitstream) name: &'a Ident,
    /// The methods' visibility.
    pub(in crate::bitstream) vis: &'a TokenStream2,
    /// The `Layout` expression the slice entry points decode in.
    pub(in crate::bitstream) layout: &'a TokenStream2,
    /// The bound on `decode`'s cursor: `Source`, or `SeekSource` for a type that seeks.
    pub(in crate::bitstream) cursor: &'a TokenStream2,
    /// `decode`'s doc. Like the four below, in the front-end's own words: what a "message"
    /// is and which layout is baked in differ between them.
    pub(in crate::bitstream) decode_doc: &'a str,
    /// `decode_all`'s doc.
    pub(in crate::bitstream) decode_all_doc: &'a str,
    /// `decode_iter`'s doc.
    pub(in crate::bitstream) decode_iter_doc: &'a str,
    /// `peek`'s doc.
    pub(in crate::bitstream) peek_doc: &'a str,
    /// `decode_exact`'s doc.
    pub(in crate::bitstream) decode_exact_doc: &'a str,
}

impl DecodeEntryPoints<'_> {
    /// The `impl` block. The two prefix decoders are stateless and worded alike everywhere:
    /// the runtime reports consumed bits, and the generated methods round up to whole bytes.
    pub(in crate::bitstream) fn expand(&self) -> TokenStream2 {
        let bnb = crate::bnb_path();
        let Self {
            name,
            vis,
            layout,
            cursor,
            decode_doc,
            decode_all_doc,
            decode_iter_doc,
            peek_doc,
            decode_exact_doc,
        } = self;
        quote! {
            impl #name {
                #[doc = #decode_doc]
                #vis fn decode<__BnbS: #cursor>(
                    __bnb_r: &mut __BnbS,
                ) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    <Self as #bnb::BitDecode>::bit_decode(__bnb_r)
                }
                #[doc = #decode_all_doc]
                #vis fn decode_all(
                    bytes: &[u8],
                ) -> ::core::result::Result<#bnb::__private::Vec<Self>, #bnb::__private::BitError> {
                    #bnb::__private::decode_all(bytes, #layout)
                }
                #[doc = #decode_iter_doc]
                #vis fn decode_iter(
                    bytes: &[u8],
                ) -> impl ::core::iter::Iterator<Item = ::core::result::Result<Self, #bnb::__private::BitError>> + '_ {
                    #bnb::__private::decode_iter(bytes, #layout)
                }
                #[doc = #peek_doc]
                #vis fn peek(bytes: &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_peek(bytes, #layout)
                }
                #[doc = #decode_exact_doc]
                #vis fn decode_exact(bytes: &[u8]) -> ::core::result::Result<Self, #bnb::__private::BitError> {
                    #bnb::__private::decode_exact(bytes, #layout)
                }
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
    }
}
