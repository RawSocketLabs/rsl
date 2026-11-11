//! `BitStreamAttrs`: the message-level options the shared generators take.

use syn::{Ident, Type};

use crate::bitstream::field::AutoLenSpec;

/// Parsed struct-level `#[bit_stream(...)]` options.
#[derive(Default)]
pub(in crate::bitstream) struct BitStreamAttrs {
    /// `allow_byte_aligned` — opt out of the right-tool guard.
    pub(in crate::bitstream) allow_byte_aligned: bool,
    /// `bits = lsb` (else MSB-first, the default).
    pub(in crate::bitstream) lsb: bool,
    /// `little` / `bytes = little` (else big-endian, the default).
    pub(in crate::bitstream) little: bool,
    /// `magic = <expr>` — a leading constant verified on read, emitted on write.
    /// Any `Bits` value, so it can even be sub-byte (`u3::new(0b110)`).
    pub(in crate::bitstream) magic: Option<syn::Expr>,
    /// `ctx(name: Ty, …)` — context this type needs from its parent. When present the
    /// type gets `decode_with`/`encode_with` (it does **not** implement
    /// `BitDecode`/`BitEncode`, which take no context).
    pub(in crate::bitstream) ctx: Vec<(Ident, Type)>,
    /// `auto_len(field.nested = kind(source), …)` — cross-struct `WireLen` derivation,
    /// applied at the targeted field's encode. Empty for the bare derives (`#[bin]` only).
    pub(in crate::bitstream) auto_len: Vec<AutoLenSpec>,
}
