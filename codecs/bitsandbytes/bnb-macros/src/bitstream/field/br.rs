//! `FieldBr`: one field's parsed `#[br]`/`#[bw]`/`#[brw]` directives, and the parser that
//! builds and validates it.

use quote::quote;
use syn::Token;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;

use super::auto_len::{AutoTarget, parse_auto_target};
use super::conflicts::check_field_directives;
use super::directive::{
    BrDirective, D_AUTO_LEN, D_BW_CALC, D_BW_MAP, D_IGNORE, D_RESERVED, D_VARIABLE, D_WRITE_WITH,
    Written,
};
use crate::bitstream::ctx::CtxArg;

/// Parsed field-level `#[br(...)]` directives.
#[derive(Clone, Default)]
#[allow(clippy::struct_excessive_bools)] // Independent presence flags in the field grammar.
pub(in crate::bitstream) struct FieldBr {
    /// `count = <expr>` — element count for a `Vec<T>` (may name an earlier field).
    pub(in crate::bitstream) count: Option<syn::Expr>,
    /// `ctx { a, b }` — pass context to a nested `ctx` message's `decode_with`/
    /// `encode_with`. Each name is a parent field or the parent's own ctx param.
    pub(in crate::bitstream) ctx: Option<Vec<CtxArg>>,
    /// `#[br(temp)]` — read into a local (usable by a later `count`/`ctx`) but do
    /// **not** store the field; `#[bin]` strips it from the struct. Pairs with
    /// `#[bw(calc = …)]` for the write side.
    pub(in crate::bitstream) temp: bool,
    /// `#[br(if(<expr>))]` — a conditional `Option<T>` field: read `Some` when the
    /// condition (over earlier fields, as locals) holds, else `None`; on encode the
    /// `Option`'s presence drives whether it is written.
    pub(in crate::bitstream) cond: Option<syn::Expr>,
    /// `#[brw(ignore)]` — a field that is **neither read nor written**: in-memory
    /// only, `Default::default()` on read (no input consumed) and skipped on write.
    /// Spelled with `brw` because it applies to both directions.
    pub(in crate::bitstream) ignore: bool,
    /// `#[brw(variable)]` — the field's type has a **variable-length** custom
    /// `BitDecode`/`BitEncode` (e.g. a `#[bin(codec = …)]` newtype): marks the parent's
    /// width indeterminate, so it never claims `FixedBitLen`. Redundant (and harmless)
    /// on a field that is already indeterminate (a `Vec`, a directive-bearing field).
    pub(in crate::bitstream) variable: bool,
    /// `#[br(map = <f>)]` — read the wire value `f`'s argument types, then `f(raw)`
    /// gives the field. `#[br(try_map = <f>)]` is the fallible form (`f` returns a
    /// `Result`); they are mutually exclusive.
    pub(in crate::bitstream) map: Option<syn::Expr>,
    pub(in crate::bitstream) try_map: Option<syn::Expr>,
    /// `#[br(parse_with = <f>)]` — the escape hatch: `f(r) -> Result<T, BitError>`
    /// reads the field with a custom function (`f: fn<S: Source>(&mut S) -> …`).
    pub(in crate::bitstream) parse_with: Option<syn::Expr>,
    /// `#[bw(calc = <expr>)]` — on encode, write `expr` (computed from the other
    /// fields) instead of `self.field`. The matched read/write pair is generated
    /// together so the directions can't drift.
    pub(in crate::bitstream) calc: Option<syn::Expr>,
    /// `#[br(calc = <expr>)]` — the read-side dual of `#[bw(calc)]`: on **decode**,
    /// bind the field to `expr` (computed from earlier fields, as locals) instead of
    /// reading wire bits, and on **encode** write nothing. The field is stored (so it
    /// is a normal builder/struct field), but it consumes zero wire bits in both
    /// directions — its bytes live in the raw fields it derives from (typically
    /// `#[br(temp)]` + `#[bw(calc)]`). This lets a later field give context to a
    /// stored typed field without a look-ahead: read the raw layout into temps in
    /// wire order, then `calc` the typed fields from the full set.
    pub(in crate::bitstream) br_calc: Option<syn::Expr>,
    /// `#[bw(map = <f>)]` — on encode, write `f(&self.field)` (the wire value).
    pub(in crate::bitstream) bw_map: Option<syn::Expr>,
    /// `#[bw(write_with = <f>)]` — the escape hatch: `f(&self.field, w) -> Result<(),
    /// BitError>` writes the field with a custom function.
    pub(in crate::bitstream) write_with: Option<syn::Expr>,
    /// `#[br(pad_before/pad_after = <bits>)]` — skip a bit count around the field
    /// (`4.bits()` / `3.bytes()` via `bnb::prelude`). `align_before/align_after`
    /// skip to the next byte boundary.
    pub(in crate::bitstream) pad_before: Option<syn::Expr>,
    pub(in crate::bitstream) pad_after: Option<syn::Expr>,
    pub(in crate::bitstream) align_before: bool,
    pub(in crate::bitstream) align_after: bool,
    /// `#[br(restore_position)]` — read the field (a peek), then rewind the cursor so
    /// later fields re-read from the same offset; skipped on write. Seeks, so the
    /// generated `decode` is bound on [`SeekSource`](bnb::SeekSource): a
    /// forward-only stream is a compile error (the slice entry points
    /// `decode`/`peek`/`decode_exact` always qualify).
    pub(in crate::bitstream) restore_position: bool,
    /// `#[br(seek = <bits>)]` — before reading, jump the cursor to that **absolute**
    /// bit offset (e.g. following a pointer). A read-side primitive (the writer is
    /// append-only); pair with `restore_position` to read at an offset and return.
    /// Like `restore_position` it seeks, so `decode` is bound on
    /// [`SeekSource`](bnb::SeekSource). On encode the seek is a no-op — see the guide.
    pub(in crate::bitstream) seek: Option<syn::Expr>,
    /// `#[br(dbg)]` — emit a `tracing` event (TRACE level, target `bnb::dbg`) carrying
    /// the field's start offset and decoded value as it is read (the field type must be
    /// `Debug`). A read-only diagnostic: it consumes no extra bits and is inert on encode.
    pub(in crate::bitstream) dbg: bool,
    /// `#[br(assert(<expr>))]` / `#[br(assert(<expr>, "fmt", args…))]` — a **decode-time
    /// guard**: after the field is read (and mapped), the expression (over this and
    /// earlier fields) must hold, else decode fails with `ErrorKind::Convert`. The
    /// *explicit opt-in* strictness escape hatch (same rejection family as `magic`,
    /// closed enums, and `try_map`: values unrepresentable in the domain) — the default
    /// stays permissive. Read-only: no `bw` inverse is needed, and encode is untouched.
    /// Multiple asserts run in order.
    pub(in crate::bitstream) asserts: Vec<(syn::Expr, Option<Punctuated<syn::Expr, Token![,]>>)>,
    /// `#[bw(auto_len = count(<field>))]` / `#[bw(auto_len = bytes(<field>))]` — the field is a
    /// [`WireLen<T>`](bnb::WireLen): on encode, an `Auto` value derives its length from the
    /// named sibling (element count, or encoded byte length), while a `Set(n)` writes `n`
    /// verbatim (dual-use). Decode reads it as `Set` transparently (via `WireLen`'s own
    /// `BitDecode`), so no `br` side is needed here. The same-struct counterpart to the
    /// struct-level `#[bin(auto_len(...))]` (which targets a *nested* field); same keyword.
    pub(in crate::bitstream) auto_len: Option<AutoTarget>,
    /// Every directive as written (`#[reserved]` included), in source order: what
    /// [`check_field_directives`] reads to reject a conflicting pair or a second, different
    /// value at the directive itself.
    seen: Vec<Written>,
}

impl FieldBr {
    /// The span of the directive `name` (a `D_*` constant) where the field carries it.
    pub(in crate::bitstream) fn span_of(&self, name: &str) -> proc_macro2::Span {
        self.seen
            .iter()
            .find(|w| w.name == name)
            .map_or_else(proc_macro2::Span::call_site, |w| w.span)
    }
}

/// A parsed item with the span of its first token (a directive's keyword), for diagnostics.
struct At<T>(proc_macro2::Span, T);

impl<T: Parse> Parse for At<T> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let span = input.span();
        Ok(At(span, input.parse()?))
    }
}

/// Parses a field's `#[br(...)]`/`#[bw(...)]`/`#[brw(...)]` directives and validates their
/// combination ([`check_field_directives`]).
pub(in crate::bitstream) fn parse_field_br(f: &syn::Field) -> syn::Result<FieldBr> {
    // The codec reads/writes through a generated source `__bnb_r` and sink `__bnb_w` — names
    // hygienic enough that a user field (even one named `r` or `w`) never shadows them, so
    // no field name is reserved.
    let mut br = FieldBr::default();
    for attr in &f.attrs {
        if attr.path().is_ident("br") {
            let directives =
                attr.parse_args_with(Punctuated::<At<BrDirective>, Token![,]>::parse_terminated)?;
            for At(span, d) in directives {
                br.seen.push(Written::new(d.name(), span, &d.value()));
                match d {
                    BrDirective::Count(e) => br.count = Some(e),
                    BrDirective::Ctx(names) => br.ctx = Some(names),
                    BrDirective::Temp => br.temp = true,
                    BrDirective::If(e) => br.cond = Some(e),
                    BrDirective::Map(e) => br.map = Some(e),
                    BrDirective::TryMap(e) => br.try_map = Some(e),
                    BrDirective::ParseWith(e) => br.parse_with = Some(e),
                    BrDirective::PadBefore(e) => br.pad_before = Some(e),
                    BrDirective::PadAfter(e) => br.pad_after = Some(e),
                    BrDirective::AlignBefore => br.align_before = true,
                    BrDirective::AlignAfter => br.align_after = true,
                    BrDirective::RestorePosition => br.restore_position = true,
                    BrDirective::Seek(e) => br.seek = Some(e),
                    BrDirective::Dbg => br.dbg = true,
                    BrDirective::Assert(cond, msg) => br.asserts.push((*cond, msg)),
                    BrDirective::Calc(e) => br.br_calc = Some(e),
                }
            }
        } else if attr.path().is_ident("bw") {
            attr.parse_nested_meta(|meta| {
                let span = meta.path.span();
                let name = if meta.path.is_ident("calc") {
                    D_BW_CALC
                } else if meta.path.is_ident("map") {
                    D_BW_MAP
                } else if meta.path.is_ident("write_with") {
                    D_WRITE_WITH
                } else if meta.path.is_ident("auto_len") {
                    D_AUTO_LEN
                } else {
                    return Err(meta.error(
                        "unknown `#[bw(...)]` directive; expected `calc = <expr>`, `map = <f>`, `write_with = <f>`, or `auto_len = count(<field>)|bytes(<field>)`",
                    ));
                };
                let value: syn::Expr = meta.value()?.parse()?;
                br.seen.push(Written::new(name, span, &value));
                match name {
                    D_BW_CALC => br.calc = Some(value),
                    D_BW_MAP => br.bw_map = Some(value),
                    D_WRITE_WITH => br.write_with = Some(value),
                    _ => br.auto_len = Some(parse_auto_target(&value)?),
                }
                Ok(())
            })?;
        } else if attr.path().is_ident("brw") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("ignore") {
                    br.seen
                        .push(Written::new(D_IGNORE, meta.path.span(), &quote!()));
                    br.ignore = true;
                    Ok(())
                } else if meta.path.is_ident("variable") {
                    br.seen
                        .push(Written::new(D_VARIABLE, meta.path.span(), &quote!()));
                    br.variable = true;
                    Ok(())
                } else if meta.path.is_ident("count_prefix") {
                    // Under `#[bin]` the desugar strips this directive before the field
                    // ever reaches this parser — so reaching it means a bare derive.
                    let _ = meta.value()?.parse::<syn::Type>()?; // consume for a clean span
                    Err(meta.error(
                        "`count_prefix` is only supported under `#[bin]` — the bare \
                         `BitDecode`/`BitEncode` derives cannot inject the length field; write \
                         the `#[br(temp)]`/`#[bw(calc = …)]`/`#[br(count = …)]` triad by hand",
                    ))
                } else {
                    Err(meta.error(
                        "unknown `#[brw(...)]` directive; expected `ignore`, `variable`, or `count_prefix = <Ty>`",
                    ))
                }
            })?;
        } else if attr.path().is_ident("reserved") || attr.path().is_ident("reserved_with") {
            br.seen
                .push(Written::new(D_RESERVED, attr.path().span(), &attr.meta));
        }
    }
    check_field_directives(&br.seen)?;
    Ok(br)
}

/// Whether a field is `#[br(temp)]` (read into a local, not stored). Used by the
/// `#[bin]` front-end ([`bin_inner`]) to filter the emitted struct/builder; the codec
/// generators read `temp` off the pre-parsed [`FieldBr`] instead.
pub(in crate::bitstream) fn field_is_temp(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| br.temp)
}

/// Whether a field is `#[brw(ignore)]` (in-memory only — defaulted on read, not
/// written, zero wire bits). Read by [`field_width`], which has no parsed `br`.
pub(in crate::bitstream) fn field_is_ignore(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| br.ignore)
}

/// Whether a field is `#[br(calc = …)]` (decode-computed, zero wire bits, not written).
/// Read by [`field_width`], which has no parsed `br`.
pub(in crate::bitstream) fn field_is_br_calc(f: &syn::Field) -> bool {
    parse_field_br(f).is_ok_and(|br| br.br_calc.is_some())
}
