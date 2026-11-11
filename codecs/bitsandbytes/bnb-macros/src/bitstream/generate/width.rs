//! Field widths: whether a message is fixed-length (`FixedBitLen`), and the bare derives'
//! right-tool alignment guard.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::FieldsNamed;

use super::options::BitStreamAttrs;
use crate::bitstream::field::{
    FieldBr, byte_array_len, field_is_br_calc, field_is_ignore, vec_elem,
};

/// The const-eval guard's message — steers the bare derive toward `#[bin]`.
const BYTE_ALIGNED_MSG: &str = "this struct's fields are all byte-aligned. The bare \
`#[derive(BitDecode/BitEncode)]` is the low-level bit codec; for a byte-aligned message use \
`#[bin]` — the unified codec (it handles byte-aligned data natively and adds \
magic/count/ctx/map/if/validate). The bare derive is for fields that straddle byte boundaries \
(e.g. a 108-bit payload). To keep the bare derive on an all-byte-aligned struct anyway, add \
`#[bit_stream(allow_byte_aligned)]`.";

/// Whether a field's directives make the message variable-length / its width
/// indeterminate, so it is exempt from the alignment guard and the message never
/// implements `FixedBitLen`: a `ctx` child (not `Bits`/`FixedBitLen`), a conditional
/// `if` (present or absent), a custom codec (`map`/`try_map`/`parse_with`/`write_with`,
/// whose wire shape lives in the converter), an explicit `#[brw(variable)]` (a
/// variable-length custom-`BitDecode` field type), or a positioning directive
/// (`pad_*`/`align_*`/`seek`/`restore_position`, which shifts the cursor).
pub(super) fn br_indeterminate(br: &FieldBr) -> bool {
    br.ctx.is_some()
        || br.cond.is_some()
        || br.map.is_some()
        || br.try_map.is_some()
        || br.bw_map.is_some()
        || br.parse_with.is_some()
        || br.write_with.is_some()
        || br.variable
        || br.pad_before.is_some()
        || br.pad_after.is_some()
        || br.align_before
        || br.align_after
        || br.restore_position
        || br.seek.is_some()
}

/// Whether `gen_decode` implements `FixedBitLen` for a message: it takes no `ctx`, no
/// directive makes a width indeterminate, and it has no `Vec` field. `gen_encode` asks
/// the same question before it names that impl to size an encode buffer.
pub(super) fn fixed_len(fields: &FieldsNamed, brs: &[FieldBr], attrs: &BitStreamAttrs) -> bool {
    attrs.ctx.is_empty()
        && !brs.iter().any(br_indeterminate)
        && !fields.named.iter().any(|f| vec_elem(f).is_some())
}

/// The bit-width expression for a field, used by the alignment guard (and, for a
/// fixed message, the `BIT_LEN` sum): a nested message contributes its
/// `FixedBitLen::BIT_LEN`, a fixed `[u8; N]` `N * 8`, a `Bits` leaf its `BITS`, a
/// `Vec<T>` its **element** width (its alignment is the element's). Resolved by
/// the compiler (the macro never computes widths).
pub(super) fn field_width(f: &syn::Field) -> TokenStream2 {
    let bnb = crate::bnb_path();
    let ty = &f.ty;
    if field_is_ignore(f) || field_is_br_calc(f) {
        return quote!(0u32); // in-memory only / decode-computed: zero wire bits
    }
    if let Some(elem) = vec_elem(f) {
        quote!(<#elem as #bnb::__private::FixedBitLen>::BIT_LEN)
    } else if let Some(len) = byte_array_len(f) {
        quote!(((#len) as u32 * 8))
    } else {
        // A leaf and a fixed message both report their width via `FixedBitLen` (leaves carry
        // the impl now: `BIT_LEN == Bits::BITS`), so no `#[nested]` is needed to size a field.
        quote!(<#ty as #bnb::__private::FixedBitLen>::BIT_LEN)
    }
}

/// A const-eval assertion that the struct is *not* entirely byte-aligned (the
/// bit-stream codec would otherwise be the wrong tool). Empty/opted-out → no guard.
/// A sub-byte `magic` counts as a non-byte-aligned element, so it suppresses the
/// guard just like a sub-byte field.
pub(super) fn alignment_guard(
    fields: &FieldsNamed,
    allow: bool,
    magic: Option<&syn::Expr>,
) -> TokenStream2 {
    if allow || (fields.named.is_empty() && magic.is_none()) {
        return quote!();
    }
    let bnb = crate::bnb_path();
    let mut terms: Vec<TokenStream2> = fields
        .named
        .iter()
        .map(|f| {
            let w = field_width(f);
            quote!((#w % 8 == 0))
        })
        .collect();
    if let Some(m) = magic {
        terms.push(quote!((#bnb::__private::bits_of(&#m) % 8 == 0)));
    }
    quote! {
        const _: () = {
            assert!(!(true #(&& #terms)*), #BYTE_ALIGNED_MSG);
        };
    }
}
