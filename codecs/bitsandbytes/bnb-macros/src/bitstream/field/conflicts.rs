//! Which field directives cannot share a field: the conflict table, the check that applies
//! it, and the accept/reject matrix that pins both.

use super::directive::{
    D_ASSERT, D_AUTO_LEN, D_BR_CALC, D_BW_CALC, D_BW_MAP, D_COUNT, D_CTX, D_IF, D_IGNORE, D_MAP,
    D_PARSE_WITH, D_RESERVED, D_RESTORE_POSITION, D_SEEK, D_TEMP, D_TRY_MAP, D_WRITE_WITH, Written,
};

/// Field directives that cannot share a field: `(directive, the directives it conflicts
/// with, why — and what to write instead)`; `$other` in the reason stands for whichever
/// listed directive the field carries. Every pair here used to drop one of the two
/// silently or make the read and write sides disagree. Pairs absent from the table compose:
/// positioning, `dbg`, `assert` and `variable` wrap or annotate any field, a read-side and a
/// write-side directive each own their direction, and `count` + `ctx` is a `Vec` of
/// context-taking elements. A rule that takes three directives to state is not a pair and
/// lives in [`check_field_directives`].
const FIELD_CONFLICTS: &[(&str, &[&str], &str)] = &[
    // `#[bw(calc = …)]` is not listed: on a stored ignored field it is the value
    // `to_canonical()` computes and `canonical_diff()` compares (a memoised field).
    (
        D_IGNORE,
        &[
            D_COUNT,
            D_CTX,
            D_TEMP,
            D_IF,
            D_MAP,
            D_TRY_MAP,
            D_PARSE_WITH,
            D_BR_CALC,
            D_BW_MAP,
            D_WRITE_WITH,
            D_AUTO_LEN,
            D_RESERVED,
        ],
        "`$other` would be ignored, because a `#[brw(ignore)]` field is neither read nor \
         written (decode leaves it at `Default::default()`). Remove `$other`, or remove \
         `ignore`",
    ),
    (
        D_BR_CALC,
        &[
            D_COUNT,
            D_CTX,
            D_TEMP,
            D_IF,
            D_MAP,
            D_TRY_MAP,
            D_PARSE_WITH,
            D_BW_CALC,
            D_BW_MAP,
            D_WRITE_WITH,
            D_AUTO_LEN,
            D_SEEK,
            D_RESTORE_POSITION,
            D_RESERVED,
        ],
        "`$other` would have nothing to act on, because `#[br(calc = …)]` computes the field \
         from earlier fields on decode and puts no bits on the wire in either direction. \
         Remove `$other` (padding and alignment may stay)",
    ),
    (
        D_IF,
        &[D_MAP, D_TRY_MAP, D_PARSE_WITH],
        "`if` reads the `Option`'s inner type with that type's own codec, so the conversion \
         would be silently dropped; put the conversion on the inner type (a `#[bin(map = …)]` \
         or `#[bin(codec = …)]` type) and keep `if` on the `Option<Inner>` field",
    ),
    (
        D_IF,
        &[D_COUNT],
        "`if` reads the `Option`'s inner type with that type's own codec, and a `Vec` has \
         none; use a plain `Vec<_>` field with `count = if <cond> { <n> } else { 0 }`",
    ),
    (
        D_IF,
        &[D_CTX],
        "`#[br(ctx { … })]` would be ignored, because `if` reads the `Option`'s inner type \
         with that type's own codec and passes it no context. Decide inside a `parse_with` \
         closure instead, which can name the earlier fields: `#[br(parse_with = |r| if <cond> \
         { <Child>::decode_with(r, <Child>Ctx { … }).map(Some) } else { Ok(None) })]`, with \
         the matching `#[bw(write_with = …)]`",
    ),
    (
        D_MAP,
        &[D_TRY_MAP],
        "keep one: `map` for an infallible conversion, `try_map` for a fallible one",
    ),
    (
        D_PARSE_WITH,
        &[D_MAP, D_TRY_MAP],
        "the `map` would win and `parse_with` would be silently dropped; do the conversion \
         inside the `parse_with` function, or drop `parse_with`",
    ),
    (
        D_COUNT,
        &[D_MAP, D_TRY_MAP, D_PARSE_WITH],
        "`#[br(count = …)]` would be ignored, because `$other` reads the whole field itself \
         (a `map` closure's argument type or the `parse_with` function decides how much). \
         Remove `count`; to size the read by an earlier field, name that field inside a \
         `parse_with` closure",
    ),
    (
        D_CTX,
        &[D_MAP, D_TRY_MAP, D_PARSE_WITH],
        "`#[br(ctx { … })]` would be ignored, because `$other` reads the whole field itself \
         and the context is never built or passed. Remove `ctx`; a `parse_with` closure can \
         name the earlier fields and hand them to the child's `decode_with` itself",
    ),
    (
        D_BW_CALC,
        &[D_BW_MAP, D_WRITE_WITH],
        "`calc` writes its value with the field type's own codec, so the custom writer would \
         be silently dropped; give the field a type whose codec is the custom one (a \
         `#[bin(codec = …)]` newtype) and compute that type in `calc`",
    ),
    (
        D_BW_CALC,
        &[D_AUTO_LEN],
        "both compute the written value, and the `calc` would win; `auto_len` already \
         derives the length, so drop the `calc`",
    ),
    (
        D_BW_MAP,
        &[D_WRITE_WITH],
        "`#[bw(map = …)]` would win and `write_with` would be silently dropped; keep one",
    ),
    (
        D_AUTO_LEN,
        &[D_BW_MAP, D_WRITE_WITH],
        "the custom writer would win and the length would never be derived; an `auto_len` \
         length is a plain `WireLen<T>` field, so drop the custom writer",
    ),
    (
        D_AUTO_LEN,
        &[D_TEMP],
        "an `auto_len` length is a stored `WireLen<T>` field; drop `temp`, or keep the \
         length unstored with `#[br(temp)]` and `#[bw(calc = …)]`",
    ),
    (
        D_RESTORE_POSITION,
        &[D_BW_MAP, D_WRITE_WITH, D_AUTO_LEN],
        "`$other` would be ignored, because a `restore_position` field is a read-side peek \
         and is never written (the field it overlaps writes those bytes). Remove `$other`; \
         a peek needs no write-side inverse, even when it is read with `map` or `parse_with`",
    ),
    (
        D_RESERVED,
        &[
            D_COUNT,
            D_CTX,
            D_IF,
            D_MAP,
            D_TRY_MAP,
            D_PARSE_WITH,
            D_BW_MAP,
            D_WRITE_WITH,
            D_AUTO_LEN,
        ],
        "`to_canonical_bytes()` writes a reserved field's spec value with the field type's \
         own plain codec, bypassing `$other`, so the canonical encoding would not be the \
         field's wire form. Remove `reserved` (a `#[builder(default = …)]` still gives the \
         field a default), or remove `$other`",
    ),
];

/// Rejects a field directive given a second, different value and the pairs in
/// [`FIELD_CONFLICTS`], spanned at the later of the two, then the rules that take three
/// directives to state. The one place a field's directive *combination* is validated. A
/// directive restated with the same value changes nothing and is accepted; `assert` may
/// repeat with any value (its guards all run, in order).
pub(super) fn check_field_directives(seen: &[Written]) -> syn::Result<()> {
    for (i, later) in seen.iter().enumerate() {
        let name = later.name;
        for earlier in &seen[..i] {
            if earlier.name == name {
                if name == D_ASSERT || earlier.value == later.value {
                    continue;
                }
                // Each `br`/`bw` directive overwrites the one before it; the reserved
                // spec value is the first attribute's.
                let message = if name == D_RESERVED {
                    format!(
                        "`{}` and `{}` both set this field's spec value, and only the first \
                         would be used; keep one",
                        earlier.reserved_form(),
                        later.reserved_form()
                    )
                } else {
                    format!(
                        "`{name}` is given twice on this field with different values, and \
                         only the last would be used; keep one"
                    )
                };
                return Err(syn::Error::new(later.span, message));
            }
            let earlier = earlier.name;
            let conflict = FIELD_CONFLICTS.iter().find(|(key, others, _)| {
                (*key == name && others.contains(&earlier))
                    || (*key == earlier && others.contains(&name))
            });
            if let Some((key, _, why)) = conflict {
                let other = if *key == name { earlier } else { name };
                let why = why.replace("$other", other);
                return Err(syn::Error::new(
                    later.span,
                    format!("`{earlier}` and `{name}` cannot be combined on one field: {why}"),
                ));
            }
        }
    }
    let span_of = |directive: &str| seen.iter().find(|w| w.name == directive).map(|w| w.span);
    let (temp, calc) = (span_of(D_TEMP), span_of(D_BW_CALC));
    // A stored peek's `calc` is live (`to_canonical()` and `canonical_diff()` recompute the
    // field); a `temp` peek has no stored value and is never written, so its `calc` is dead.
    if let (Some(_), Some(_), Some(calc)) = (span_of(D_RESTORE_POSITION), temp, calc) {
        return Err(syn::Error::new(
            calc,
            format!(
                "`{D_BW_CALC}` would never run on a field that is both `{D_TEMP}` and \
                 `{D_RESTORE_POSITION}`: a peek is never written and a `temp` one is not \
                 stored, so nothing uses the value. Remove the `calc` (a `temp` peek needs \
                 none), or remove `temp` to store the peek and let `to_canonical()` \
                 recompute it"
            ),
        ));
    }
    // `reserved` names the value the canonical encoder writes and `calc` the one the
    // verbatim encoder writes. On an unstored (`temp`) field both hold; with only one of
    // `temp` and `calc` beside it, `reserved` has nothing to act on or contradicts the `calc`.
    if span_of(D_RESERVED).is_some() {
        match (temp, calc) {
            (Some(temp), None) => {
                return Err(syn::Error::new(
                    temp,
                    format!(
                        "`{D_RESERVED}` is ignored on a `{D_TEMP}` field that has no \
                         `{D_BW_CALC}`: the field is not stored, so there is no value for \
                         the reserved spec value to stand in for. Remove `reserved`, or add \
                         `{D_BW_CALC}` for the value `to_bytes` writes (`to_canonical_bytes` \
                         then writes the reserved one)"
                    ),
                ));
            }
            (None, Some(calc)) => {
                return Err(syn::Error::new(
                    calc,
                    format!(
                        "`{D_RESERVED}` and `{D_BW_CALC}` give a stored field two canonical \
                         values: `to_canonical_bytes()` writes the reserved spec value, but \
                         `to_canonical()` stores the `calc`. Keep `reserved` for a fixed \
                         value or `calc` for a computed one, or add `{D_TEMP}` so the field \
                         is not stored"
                    ),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The whole accept/reject matrix of a field's directives, stated independently of
/// [`FIELD_CONFLICTS`] and driven through the real parser on attributes as a user writes
/// them: losing a rejection, adding one, or recording one directive as another fails here,
/// naming the pair.
#[cfg(test)]
mod directive_tests {
    use proc_macro2::TokenStream as TokenStream2;

    use super::FIELD_CONFLICTS;
    use crate::bitstream::field::br::parse_field_br;
    use crate::bitstream::field::count_prefix::extract_count_prefix;
    use crate::bitstream::field::directive::{
        D_ALIGN_AFTER, D_ALIGN_BEFORE, D_ASSERT, D_AUTO_LEN, D_BR_CALC, D_BW_CALC, D_BW_MAP,
        D_COUNT, D_CTX, D_DBG, D_IF, D_IGNORE, D_MAP, D_PAD_AFTER, D_PAD_BEFORE, D_PARSE_WITH,
        D_RESERVED, D_RESTORE_POSITION, D_SEEK, D_TEMP, D_TRY_MAP, D_VARIABLE, D_WRITE_WITH,
    };

    /// Every field directive: an attribute that carries it and, where it takes a value, the
    /// same attribute with another value.
    const ALL: &[(&str, &str, Option<&str>)] = &[
        (D_COUNT, "#[br(count = n)]", Some("#[br(count = m)]")),
        (D_CTX, "#[br(ctx { n })]", Some("#[br(ctx { m })]")),
        (D_TEMP, "#[br(temp)]", None),
        (D_IF, "#[br(if(n > 0))]", Some("#[br(if(m > 0))]")),
        (D_MAP, "#[br(map = f)]", Some("#[br(map = g)]")),
        (D_TRY_MAP, "#[br(try_map = f)]", Some("#[br(try_map = g)]")),
        (
            D_PARSE_WITH,
            "#[br(parse_with = f)]",
            Some("#[br(parse_with = g)]"),
        ),
        (
            D_PAD_BEFORE,
            "#[br(pad_before = n)]",
            Some("#[br(pad_before = m)]"),
        ),
        (
            D_PAD_AFTER,
            "#[br(pad_after = n)]",
            Some("#[br(pad_after = m)]"),
        ),
        (D_ALIGN_BEFORE, "#[br(align_before)]", None),
        (D_ALIGN_AFTER, "#[br(align_after)]", None),
        (D_RESTORE_POSITION, "#[br(restore_position)]", None),
        (D_SEEK, "#[br(seek = n)]", Some("#[br(seek = m)]")),
        (D_DBG, "#[br(dbg)]", None),
        (
            D_ASSERT,
            "#[br(assert(n > 0))]",
            Some("#[br(assert(m > 0))]"),
        ),
        (D_BR_CALC, "#[br(calc = n)]", Some("#[br(calc = m)]")),
        (D_BW_CALC, "#[bw(calc = n)]", Some("#[bw(calc = m)]")),
        (D_BW_MAP, "#[bw(map = f)]", Some("#[bw(map = g)]")),
        (
            D_WRITE_WITH,
            "#[bw(write_with = f)]",
            Some("#[bw(write_with = g)]"),
        ),
        (
            D_AUTO_LEN,
            "#[bw(auto_len = count(n))]",
            Some("#[bw(auto_len = count(m))]"),
        ),
        (D_IGNORE, "#[brw(ignore)]", None),
        (D_VARIABLE, "#[brw(variable)]", None),
        (D_RESERVED, "#[reserved]", Some("#[reserved_with(1)]")),
    ];

    /// The pairs that are a compile error, each once: before the check existed, one of the
    /// two was never compiled, or the two encoded different bytes. Every other pair composes.
    const REJECTED: &[(&str, &str)] = &[
        // A mapped or custom-parsed field is read whole: its count or context is dropped.
        (D_COUNT, D_MAP),
        (D_COUNT, D_TRY_MAP),
        (D_COUNT, D_PARSE_WITH),
        (D_CTX, D_MAP),
        (D_CTX, D_TRY_MAP),
        (D_CTX, D_PARSE_WITH),
        // `if` reads the `Option`'s inner type with that type's own codec.
        (D_IF, D_COUNT),
        (D_IF, D_CTX),
        (D_IF, D_MAP),
        (D_IF, D_TRY_MAP),
        (D_IF, D_PARSE_WITH),
        // One reader, one writer per field.
        (D_MAP, D_TRY_MAP),
        (D_MAP, D_PARSE_WITH),
        (D_TRY_MAP, D_PARSE_WITH),
        (D_BW_MAP, D_WRITE_WITH),
        (D_BW_CALC, D_BW_MAP),
        (D_BW_CALC, D_WRITE_WITH),
        (D_BW_CALC, D_AUTO_LEN),
        (D_AUTO_LEN, D_BW_MAP),
        (D_AUTO_LEN, D_WRITE_WITH),
        (D_AUTO_LEN, D_TEMP),
        // A peek is never written.
        (D_RESTORE_POSITION, D_BW_MAP),
        (D_RESTORE_POSITION, D_WRITE_WITH),
        (D_RESTORE_POSITION, D_AUTO_LEN),
        // `br(calc)` puts no bits on the wire: the pre-table list, plus `reserved`.
        (D_BR_CALC, D_COUNT),
        (D_BR_CALC, D_CTX),
        (D_BR_CALC, D_TEMP),
        (D_BR_CALC, D_IF),
        (D_BR_CALC, D_MAP),
        (D_BR_CALC, D_TRY_MAP),
        (D_BR_CALC, D_PARSE_WITH),
        (D_BR_CALC, D_BW_CALC),
        (D_BR_CALC, D_BW_MAP),
        (D_BR_CALC, D_WRITE_WITH),
        (D_BR_CALC, D_AUTO_LEN),
        (D_BR_CALC, D_IGNORE),
        (D_BR_CALC, D_SEEK),
        (D_BR_CALC, D_RESTORE_POSITION),
        (D_BR_CALC, D_RESERVED),
        // An ignored field is neither read nor written. (Its `bw(calc)` is live in the
        // canonical helpers, so that pair composes.)
        (D_IGNORE, D_COUNT),
        (D_IGNORE, D_CTX),
        (D_IGNORE, D_TEMP),
        (D_IGNORE, D_IF),
        (D_IGNORE, D_MAP),
        (D_IGNORE, D_TRY_MAP),
        (D_IGNORE, D_PARSE_WITH),
        (D_IGNORE, D_BW_MAP),
        (D_IGNORE, D_WRITE_WITH),
        (D_IGNORE, D_AUTO_LEN),
        (D_IGNORE, D_RESERVED),
        // The canonical encoder writes a reserved value with the type's plain codec. With
        // `temp` or `bw(calc)` alone it has nothing to stand in for, or contradicts the
        // `calc` (the three together are accepted: `three_directive_rules`).
        (D_RESERVED, D_COUNT),
        (D_RESERVED, D_CTX),
        (D_RESERVED, D_IF),
        (D_RESERVED, D_MAP),
        (D_RESERVED, D_TRY_MAP),
        (D_RESERVED, D_PARSE_WITH),
        (D_RESERVED, D_BW_MAP),
        (D_RESERVED, D_WRITE_WITH),
        (D_RESERVED, D_AUTO_LEN),
        (D_RESERVED, D_TEMP),
        (D_RESERVED, D_BW_CALC),
    ];

    /// The attribute in [`ALL`] that carries the directive `name`.
    fn sample(name: &str) -> &'static str {
        let found = ALL.iter().find(|(directive, ..)| *directive == name);
        found
            .unwrap_or_else(|| panic!("`{name}` is missing from `ALL`"))
            .1
    }

    /// Whether a field carrying `attrs`, in that order, gets through the checks `#[bin]`
    /// runs on a field's directives: the `count_prefix` scan, then the parser.
    fn accepted(attrs: &[&str]) -> bool {
        let attrs = attrs.iter().map(|attr| {
            attr.parse::<TokenStream2>()
                .expect("a sample attribute is valid tokens")
        });
        let mut field: syn::Field = syn::parse_quote!(#(#attrs)* field: u8);
        extract_count_prefix(&mut field)
            .and_then(|_| parse_field_br(&field))
            .is_ok()
    }

    fn listed(a: &str, b: &str) -> bool {
        REJECTED
            .iter()
            .any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
    }

    #[test]
    fn every_directive_pair_is_accepted_or_rejected_as_listed() {
        for (key, others, _) in FIELD_CONFLICTS {
            for name in others.iter().chain([key]) {
                sample(name);
            }
        }
        for (a, b) in REJECTED {
            sample(a);
            sample(b);
        }
        let mut wrong = Vec::new();
        for &(a, a_attr, _) in ALL {
            for &(b, b_attr, _) in ALL.iter().filter(|(b, ..)| *b != a) {
                let ok = accepted(&[a_attr, b_attr]);
                if ok == listed(a, b) {
                    let verdict = if ok { "accepted" } else { "rejected" };
                    wrong.push(format!("`{a}` then `{b}` is {verdict}"));
                }
            }
        }
        assert!(
            wrong.is_empty(),
            "pairs that contradict `REJECTED`:\n{}",
            wrong.join("\n")
        );
    }

    #[test]
    fn a_restated_directive_is_accepted_and_a_second_value_rejected() {
        for &(name, attr, other_value) in ALL {
            assert!(accepted(&[attr]), "`{name}` alone is rejected");
            assert!(accepted(&[attr, attr]), "`{name}` restated is rejected");
            let Some(other_value) = other_value else {
                continue;
            };
            assert_eq!(
                accepted(&[attr, other_value]),
                name == D_ASSERT,
                "`{name}` given two values: only `assert` may repeat"
            );
        }
    }

    #[test]
    fn three_directive_rules() {
        let orders = |[a, b, c]: [&'static str; 3]| {
            [
                [a, b, c],
                [a, c, b],
                [b, a, c],
                [b, c, a],
                [c, a, b],
                [c, b, a],
            ]
            .map(|order| order.map(sample))
        };
        for order in orders([D_RESERVED, D_TEMP, D_BW_CALC]) {
            assert!(
                accepted(&order),
                "an unstored reserved `calc` field is rejected: {order:?}"
            );
        }
        for order in orders([D_RESTORE_POSITION, D_TEMP, D_BW_CALC]) {
            assert!(
                !accepted(&order),
                "a `temp` peek's dead `calc` is accepted: {order:?}"
            );
        }
        assert!(
            accepted(&[sample(D_RESTORE_POSITION), sample(D_BW_CALC)]),
            "a stored peek's `calc` is rejected"
        );
    }
}
