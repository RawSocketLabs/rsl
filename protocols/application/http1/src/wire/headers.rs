//! Header fields.

/// One header field: its name and value as bytes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Header {
    /// The field name, in the case it was written.
    pub name: Vec<u8>,
    /// The field value, without the optional whitespace around it.
    pub value: Vec<u8>,
}

/// The header fields of a message, in order, duplicates included. Names compare without
/// regard to ASCII case (RFC 9110 §5.1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Headers {
    /// The fields, in message order.
    pub fields: Vec<Header>,
}

impl Headers {
    /// No fields.
    #[must_use]
    pub const fn new() -> Self {
        Self { fields: Vec::new() }
    }

    /// Appends a field (no validation).
    pub fn push(&mut self, name: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) {
        self.fields.push(Header {
            name: name.into(),
            value: value.into(),
        });
    }

    /// The value of the first field named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.fields
            .iter()
            .find(|field| field.name.eq_ignore_ascii_case(name.as_bytes()))
            .map(|field| field.value.as_slice())
    }

    /// The value of every field named `name`, in order.
    pub fn get_all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a [u8]> + 'a {
        self.fields
            .iter()
            .filter(move |field| field.name.eq_ignore_ascii_case(name.as_bytes()))
            .map(|field| field.value.as_slice())
    }

    /// Every element of the comma-separated lists in the fields named `name`, trimmed, in
    /// order (RFC 9110 §5.6.1). Empty elements are skipped.
    pub fn list<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a [u8]> + 'a {
        self.get_all(name)
            .flat_map(|value| value.split(|&byte| byte == b','))
            .map(trim_ows)
            .filter(|element| !element.is_empty())
    }

    /// Whether any list element in the fields named `name` equals `token`, ignoring case.
    #[must_use]
    pub fn has_token(&self, name: &str, token: &str) -> bool {
        self.list(name)
            .any(|element| element.eq_ignore_ascii_case(token.as_bytes()))
    }
}

/// `bytes` without the optional whitespace around it. That is space and horizontal tab only
/// (RFC 9110 §5.6.3): a form feed or vertical tab is part of the value, where it makes a
/// length or a coding name invalid rather than being quietly dropped.
pub(super) fn trim_ows(bytes: &[u8]) -> &[u8] {
    let is_ows = |byte: &u8| matches!(byte, b' ' | b'\t');
    let start = bytes
        .iter()
        .position(|byte| !is_ows(byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !is_ows(byte))
        .map_or(start, |last| last + 1);
    &bytes[start..end]
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn only_space_and_tab_are_trimmed() {
        assert_eq!(trim_ows(b" \t a b\t "), b"a b");
        assert_eq!(trim_ows(b"\x0c5\x0b"), b"\x0c5\x0b");
        assert_eq!(trim_ows(b" \t "), b"");
        assert_eq!(trim_ows(b""), b"");
    }
}
