//! One cookie.

use alloc::vec::Vec;

use crate::date;

/// One cookie as a client stores it: its name and value, the path it is sent for, and when
/// it expires.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Cookie {
    /// The name.
    pub name: Vec<u8>,
    /// The value.
    pub value: Vec<u8>,
    /// The path prefix of the requests the cookie is sent with.
    pub path: Vec<u8>,
    /// When the cookie expires, in seconds since the Unix epoch, or `None` for a cookie that
    /// lasts until the caller discards the jar.
    pub expires: Option<i64>,
}

impl Cookie {
    /// A cookie for every path that never expires, as when restoring a saved session.
    #[must_use]
    pub fn new(name: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            path: b"/".to_vec(),
            expires: None,
        }
    }

    /// Parses the value of one `Set-Cookie` header by the user-agent algorithm of RFC 6265
    /// §5.2, which is more forgiving than the grammar servers are asked to follow.
    ///
    /// `request_path` is the path of the request the response answers; it gives the cookie
    /// its path when the header names none (§5.1.4). `now` is the current time in seconds
    /// since the Unix epoch, which `Max-Age` counts from. `Max-Age` wins over `Expires`, and
    /// the last of a repeated attribute wins. `Domain`, `Secure`, `HttpOnly` and unknown
    /// attributes are ignored.
    ///
    /// Returns `None` for a header the algorithm says to ignore: no `=` in the first part,
    /// or an empty name.
    #[must_use]
    pub fn parse(set_cookie: &[u8], request_path: &[u8], now: i64) -> Option<Self> {
        let mut parts = set_cookie.split(|&byte| byte == b';');
        let (name, value) = split_pair(parts.next()?)?;
        if name.is_empty() {
            return None;
        }
        let (mut path, mut expires, mut max_age) = (None, None, None);
        for attribute in parts {
            let (key, argument) = split_pair(attribute).unwrap_or((trim(attribute), b""));
            if key.eq_ignore_ascii_case(b"Path") {
                path = Some(argument).filter(|path| path.starts_with(b"/"));
            } else if key.eq_ignore_ascii_case(b"Expires") {
                expires = date::parse(argument).or(expires);
            } else if key.eq_ignore_ascii_case(b"Max-Age") {
                max_age = max_age_expiry(argument, now).or(max_age);
            }
        }
        Some(Self {
            name: name.to_vec(),
            value: value.to_vec(),
            path: path.map_or_else(|| default_path(request_path), <[u8]>::to_vec),
            expires: max_age.or(expires),
        })
    }

    /// Whether the cookie has expired at `now`.
    #[must_use]
    pub fn is_expired(&self, now: i64) -> bool {
        self.expires.is_some_and(|expires| expires <= now)
    }

    /// Whether the cookie is sent with a request for `request_path` (RFC 6265 §5.1.4,
    /// path-match). A query string on `request_path` is ignored.
    #[must_use]
    pub fn matches_path(&self, request_path: &[u8]) -> bool {
        let request_path = without_query(request_path);
        let Some(rest) = request_path.strip_prefix(self.path.as_slice()) else {
            return false;
        };
        rest.is_empty() || self.path.ends_with(b"/") || rest.starts_with(b"/")
    }
}

/// `bytes` without leading and trailing spaces and tabs.
fn trim(bytes: &[u8]) -> &[u8] {
    let is_wsp = |byte: &u8| matches!(byte, b' ' | b'\t');
    let start = bytes
        .iter()
        .position(|byte| !is_wsp(byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !is_wsp(byte))
        .map_or(start, |last| last + 1);
    &bytes[start..end]
}

/// Splits `name=value` at its first `=`, trimming both sides.
fn split_pair(pair: &[u8]) -> Option<(&[u8], &[u8])> {
    let equals = pair.iter().position(|&byte| byte == b'=')?;
    Some((trim(&pair[..equals]), trim(&pair[equals + 1..])))
}

/// The expiry a `Max-Age` value gives (RFC 6265 §5.2.2): an optional minus sign, then digits
/// only. Zero or less means already expired.
fn max_age_expiry(value: &[u8], now: i64) -> Option<i64> {
    let digits = value.strip_prefix(b"-").unwrap_or(value);
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if value.starts_with(b"-") || digits.iter().all(|&digit| digit == b'0') {
        return Some(i64::MIN);
    }
    // A delta too large to represent is as good as never.
    let delta = digits.iter().try_fold(0_i64, |delta, digit| {
        delta.checked_mul(10)?.checked_add(i64::from(digit - b'0'))
    });
    Some(
        delta
            .and_then(|delta| now.checked_add(delta))
            .unwrap_or(i64::MAX),
    )
}

/// `path` up to a query string.
fn without_query(path: &[u8]) -> &[u8] {
    let end = path
        .iter()
        .position(|&byte| byte == b'?')
        .unwrap_or(path.len());
    &path[..end]
}

/// The path a cookie gets when its header names none (RFC 6265 §5.1.4): the request path up
/// to its last `/`, or `/`.
fn default_path(request_path: &[u8]) -> Vec<u8> {
    let path = without_query(request_path);
    let last_slash = path.iter().rposition(|&byte| byte == b'/');
    match last_slash {
        Some(at) if at > 0 && path.starts_with(b"/") => path[..at].to_vec(),
        _ => b"/".to_vec(),
    }
}
