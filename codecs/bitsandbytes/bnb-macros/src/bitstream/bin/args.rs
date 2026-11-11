//! `BinArgs`: the parsed `#[bin(...)]` options.

use syn::{Ident, Type};

use crate::bitstream::field::AutoLenSpec;

/// Parsed struct-level `#[bin(...)]` options.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)] // Independent syntax options, not a runtime state machine.
pub(super) struct BinArgs {
    pub(super) read_only: bool,
    pub(super) write_only: bool,
    pub(super) no_builder: bool,
    pub(super) forward_only: bool,
    pub(super) lsb: bool,
    pub(super) little: bool,
    pub(super) magic: Option<syn::Expr>,
    pub(super) ctx: Vec<(Ident, Type)>,
    /// `auto_len(<field>.<nested> = count|bytes(<source>), …)` — cross-struct
    /// [`WireLen`](bnb::WireLen) derivation: on encode, resolve each nested `Auto` length
    /// from a sibling collection (DNS `header.qdcount = count(questions)`). A `Set` value is
    /// still honored (dual-use).
    pub(super) auto_len: Vec<AutoLenSpec>,
    /// `validate = <path>` — a `fn(&Self) -> Result<(), impl Display>` run by
    /// `build()` (construction soundness; the parser stays permissive). A free
    /// function, not a method, so it isn't mistaken for protocol-context validity.
    pub(super) validate: Option<syn::Path>,
    /// `tag = <ctx-param>` (enum only) — the **selector**: dispatch each `#[bin(tag =
    /// <value>)]` variant on this `ctx(...)` parameter (read-only, never on the wire).
    pub(super) tag: Option<Ident>,
    /// `map = |w: Wire| Self` (struct only) — the logical type's wire form is the message
    /// type `Wire`; decode reads `Wire` then maps it. The wire type is taken from the
    /// closure's annotated parameter.
    pub(super) map: Option<syn::Expr>,
    /// `try_map = |w: Wire| Result<Self, E>` — the fallible form of [`map`](Self::map).
    pub(super) try_map: Option<syn::Expr>,
    /// `bw_map = |s: &Self| Wire` (struct only) — the encode dual: map the logical type to
    /// its wire form, then encode that.
    pub(super) bw_map: Option<syn::Expr>,
    /// `wire = WireType` (struct only) — the conversion-trait form of [`map`](Self::map):
    /// decode reads `WireType` then `Self::from(wire)` (needs `From<WireType> for Self`), encode
    /// writes `WireType::from(&self)` (needs `From<&Self> for WireType`). The transitions live in
    /// the user's `impl From` blocks (a clean home, reusable in-program).
    pub(super) wire: Option<Type>,
    /// `try_wire = WireType` — the fallible form of [`wire`](Self::wire): decode is
    /// `Self::try_from(wire)` (needs `TryFrom<WireType> for Self`, `Error: Display`).
    pub(super) try_wire: Option<Type>,
    /// `codec = <module>` (desugared at parse time to `<module>::parse` /
    /// `<module>::write`) or `codec(parse = <f>, write = <f>)` — a **per-type codec**:
    /// this single-field tuple struct's wire form is owned by the fn pair (the reusable
    /// dual of the per-field `parse_with`/`write_with`). Either fn may be absent under
    /// the paren form when `read_only`/`write_only` narrows the direction.
    pub(super) codec_parse: Option<syn::Expr>,
    pub(super) codec_write: Option<syn::Expr>,
    /// Every option as written, with its span: what [`BinArgs::reject`] reads to error at
    /// an option the item's kind cannot honour.
    pub(super) seen: Vec<(String, proc_macro2::Span)>,
}

impl BinArgs {
    /// Errors, at the option itself, for the first of `options` this `#[bin(...)]` carries:
    /// `why` completes the sentence "`<option>` …", saying where the option belongs. For an
    /// item kind that would otherwise parse the option and ignore it.
    pub(super) fn reject(&self, options: &[&str], why: &str) -> syn::Result<()> {
        match self
            .seen
            .iter()
            .find(|(name, _)| options.contains(&name.as_str()))
        {
            Some((name, span)) => Err(syn::Error::new(*span, format!("`{name}` {why}"))),
            None => Ok(()),
        }
    }

    /// Whether any wire-mapping option is set (the struct serializes via a separate wire type
    /// rather than its own fields) — either the closure form (`map`/`try_map`/`bw_map`) or the
    /// conversion-trait form (`wire`/`try_wire`).
    pub(super) fn is_mapped(&self) -> bool {
        self.map.is_some()
            || self.try_map.is_some()
            || self.bw_map.is_some()
            || self.wire.is_some()
            || self.try_wire.is_some()
    }

    /// Whether a per-type codec is set (either form, either direction).
    pub(super) fn has_codec(&self) -> bool {
        self.codec_parse.is_some() || self.codec_write.is_some()
    }
}
