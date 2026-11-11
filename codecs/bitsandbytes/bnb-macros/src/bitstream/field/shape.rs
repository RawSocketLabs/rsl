//! Field type shapes the codec treats specially: `[u8; N]`, `Vec<T>` and `Option<T>`.

// `#[nested]` is now obsolete: every field type (a `Bits` leaf or a message) implements
// `BitDecode`/`BitEncode`, so `#[bin]` decodes/encodes and sizes every field uniformly with no
// marker. The attribute is still accepted (and stripped by `is_codec_field_attr`) for backward
// compatibility, but it no longer affects codegen.

/// If the field is a fixed `[u8; N]` byte array, returns its length expression. Seen
/// through a `macro_rules!` type fragment, like every shape check here.
pub(in crate::bitstream) fn byte_array_len(f: &syn::Field) -> Option<&syn::Expr> {
    if let syn::Type::Array(arr) = crate::ungroup(&f.ty) {
        if crate::is_bare_ident(&arr.elem, "u8") {
            return Some(&arr.len);
        }
    }
    None
}

/// If the field's type is `Vec<T>`, returns the element type `T` — a
/// variable-length, `count`-driven field.
pub(in crate::bitstream) fn vec_elem(f: &syn::Field) -> Option<&syn::Type> {
    single_generic(&f.ty, "Vec")
}

/// If the field's type is `Option<T>`, returns the inner type `T` — a
/// conditional (`#[br(if(...))]`) field.
pub(in crate::bitstream) fn option_elem(f: &syn::Field) -> Option<&syn::Type> {
    single_generic(&f.ty, "Option")
}

/// The single type argument of `Wrapper<T>`, if `ty` is `Wrapper<T>`.
fn single_generic<'a>(ty: &'a syn::Type, wrapper: &str) -> Option<&'a syn::Type> {
    if let syn::Type::Path(p) = crate::ungroup(ty) {
        let seg = p.path.segments.last()?;
        if seg.ident == wrapper {
            if let syn::PathArguments::AngleBracketed(a) = &seg.arguments {
                if let Some(syn::GenericArgument::Type(t)) = a.args.first() {
                    return Some(t);
                }
            }
        }
    }
    None
}
