//! The request a client sends.

// --- Internal modules ---
use super::{Headers, headers::trim_ows};
use crate::Error;

/// One HTTP/1.1 request (RFC 9112 §3).
///
/// Dual-use: every field is public and [`encode`](Self::encode) writes them verbatim, so a
/// caller can send any method, target, header or body, a `Content-Length` that disagrees
/// with the body included. The constructors and [`header`](Self::header) are the guided
/// path: they refuse bytes that would end a line early and so change what the server parses.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Request {
    /// The method, such as `GET`. Case matters: `get` is a different method.
    pub method: Vec<u8>,
    /// The request target, such as `/path?query`.
    pub target: Vec<u8>,
    /// The protocol version on the request line. The constructors set `HTTP/1.1`.
    pub version: Vec<u8>,
    /// The header fields, in the order they are sent.
    pub headers: Headers,
    /// The body. Nothing describes its length to the server but the header fields.
    pub body: Vec<u8>,
}

impl Request {
    /// A `GET` of `target` with the `Host` header a server requires (RFC 9112 §3.2). `host`
    /// includes the port when it is not the scheme's default.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidRequest`] when `host` or `target` contains a byte that cannot appear
    /// there.
    pub fn get(host: &str, target: &str) -> Result<Self, Error> {
        Self::new("GET", host, target)
    }

    /// A `POST` of `body` to `target`, with `Host`, `Content-Type` and the `Content-Length`
    /// of `body`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidRequest`] when `host`, `target` or `content_type` contains a byte
    /// that cannot appear there.
    pub fn post(
        host: &str,
        target: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> Result<Self, Error> {
        let length = body.len().to_string();
        let mut request = Self::new("POST", host, target)?
            .header("Content-Type", content_type)?
            .header("Content-Length", &length)?;
        request.body = body;
        Ok(request)
    }

    /// A request with this method, target and `Host`, and no body.
    fn new(method: &str, host: &str, target: &str) -> Result<Self, Error> {
        if !is_visible_target(target.as_bytes()) {
            return Err(Error::InvalidRequest(
                "the target is empty or contains a space, control or non-ASCII byte",
            ));
        }
        let request = Self {
            method: method.into(),
            target: target.into(),
            version: b"HTTP/1.1".to_vec(),
            headers: Headers::new(),
            body: Vec::new(),
        };
        request.header("Host", host)
    }

    /// Adds a header field. It refuses what would change how a server frames or routes the
    /// request:
    ///
    /// - a name that is not a token, or a value containing a carriage return, line feed or
    ///   NUL, which would let the value end its own line and inject a field or a request;
    /// - a second `Host`, `Content-Length` or `Transfer-Encoding`, and either of the last two
    ///   when the other is present, because servers disagree about which one counts.
    ///
    /// To send any of those anyway, push onto [`headers`](Self::headers) directly.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidRequest`] for such a name, value or repetition.
    pub fn header(mut self, name: &str, value: &str) -> Result<Self, Error> {
        if !is_token(name.as_bytes()) {
            return Err(Error::InvalidRequest("a header name is not a token"));
        }
        let has = |field: &str| self.headers.get(field).is_some();
        let is = |field: &str| name.eq_ignore_ascii_case(field);
        let framing = is("Content-Length") || is("Transfer-Encoding");
        if (is("Host") && has("Host"))
            || (framing && (has("Content-Length") || has("Transfer-Encoding")))
        {
            return Err(Error::InvalidRequest(
                "a second Host, or a second body-length field, is ambiguous to servers",
            ));
        }
        if value.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | 0)) {
            return Err(Error::InvalidRequest(
                "a header value contains a carriage return, line feed or NUL",
            ));
        }
        self.headers.push(name, value.trim_matches([' ', '\t']));
        Ok(self)
    }

    /// Checks that the raw fields encode one HTTP/1.1 request with a fixed-size body.
    /// Call this before sending modifiable request fields on a transport that requires
    /// unambiguous framing. The caller still chooses the permitted Host and target form.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidRequest`] for invalid request-line or field syntax, a transfer
    /// coding, or a missing, repeated or mismatching body length. Empty bodies may omit
    /// `Content-Length`.
    pub fn validate_fixed_length(&self) -> Result<(), Error> {
        if !is_token(&self.method)
            || !is_visible_target(&self.target)
            || self.version != b"HTTP/1.1"
        {
            return Err(Error::InvalidRequest("invalid HTTP/1.1 request line"));
        }
        let mut length = None;
        for field in &self.headers.fields {
            if !is_token(&field.name)
                || field
                    .value
                    .iter()
                    .any(|&b| b.is_ascii_control() && b != b'\t')
            {
                return Err(Error::InvalidRequest("invalid header field syntax"));
            }
            if field.name.eq_ignore_ascii_case(b"Transfer-Encoding") {
                return Err(Error::InvalidRequest(
                    "a fixed-length request cannot use transfer coding",
                ));
            }
            if field.name.eq_ignore_ascii_case(b"Content-Length") {
                let value = trim_ows(&field.value);
                if length.is_some() || value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
                    return Err(Error::InvalidRequest(
                        "expected a single decimal Content-Length",
                    ));
                }
                length = Some(
                    value
                        .iter()
                        .try_fold(0_usize, |n, b| {
                            n.checked_mul(10)?.checked_add(usize::from(b - b'0'))
                        })
                        .ok_or(Error::InvalidRequest("Content-Length overflows"))?,
                );
            }
        }
        if self.body.len() != length.unwrap_or(0) {
            return Err(Error::InvalidRequest(
                "Content-Length does not match the request body",
            ));
        }
        Ok(())
    }

    /// The request as the bytes to send: request line, header fields, a blank line, the body.
    /// Nothing is checked or added.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut wire = Vec::with_capacity(128 + self.body.len());
        wire.extend(&self.method);
        wire.push(b' ');
        wire.extend(&self.target);
        wire.push(b' ');
        wire.extend(&self.version);
        wire.extend(b"\r\n");
        for field in &self.headers.fields {
            wire.extend(&field.name);
            wire.extend(b": ");
            wire.extend(&field.value);
            wire.extend(b"\r\n");
        }
        wire.extend(b"\r\n");
        wire.extend(&self.body);
        wire
    }
}

/// A method or field name consists of one or more token bytes.
fn is_token(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().copied().all(is_token_byte)
}

/// A target occupies one nonempty field of the request line.
fn is_visible_target(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_graphic)
}

/// Whether `byte` may appear in a token, the syntax of methods and field names
/// (RFC 9110 §5.6.2).
pub(super) const fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'..=b'\'' | b'*' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
        )
}

#[cfg(test)]
mod unit {
    use super::*;

    /// RFC 9110 §5.6.2: `tchar` is a letter, a digit, or one of these fifteen marks.
    #[test]
    fn token_bytes_are_exactly_tchar() {
        let marks = b"!#$%&'*+-.^_`|~";
        for byte in 0..=u8::MAX {
            let expected = byte.is_ascii_alphanumeric() || marks.contains(&byte);
            assert_eq!(is_token_byte(byte), expected, "byte {byte:#04x}");
        }
    }
}
