//! Reading members of a JSON value whose key spelling is not fixed.

// --- Workspace dependencies ---
use serde_json::Value;

/// The member `key` of a JSON object, matched without regard to letter case (spec §6.1).
pub(super) fn member<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    value?
        .as_object()?
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, member)| member)
}

/// The string member `key`, or the empty string when it is missing or not a string.
pub(super) fn text<'a>(value: Option<&'a Value>, key: &str) -> &'a str {
    member(value, key).and_then(Value::as_str).unwrap_or("")
}

/// The array member `key`, or no items when it is missing or not an array.
pub(super) fn items<'a>(value: Option<&'a Value>, key: &str) -> &'a [Value] {
    member(value, key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}
