//! How a response's body is delimited.

// --- Internal modules ---
use super::ResponseHead;
use crate::Error;

/// How the end of a response's body is found (RFC 9112 §6.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyFraming {
    /// There is no body: the message ends with its head.
    None,
    /// The body is exactly this many bytes.
    Length(u64),
    /// The body is chunked (§7.1).
    Chunked,
    /// The body runs until the server closes the connection.
    UntilClose,
}

impl BodyFraming {
    /// Decides how the body of the response `head` to a `request_method` request is
    /// delimited, following the order of precedence in RFC 9112 §6.3.
    ///
    /// # Errors
    ///
    /// [`Error::Framing`] when the head leaves the length ambiguous: both `Transfer-Encoding`
    /// and `Content-Length` (rule 3, which the RFC says ought to be treated as an error), or
    /// a `Content-Length` that is not one decimal number, repeated or not (rule 5).
    /// [`Error::Unsupported`] for a transfer coding other than `chunked` alone, and for a
    /// successful response to `CONNECT`, which turns the connection into a tunnel.
    //~ rejects rfc9112#6.3 reason="rule 3 lets Transfer-Encoding override Content-Length and says it ought to be an error; it is one here"
    //~ deviates rfc9112#6.3 reason="rule 4 reads a non-final-chunked body to close; it is refused instead, because a coding this crate cannot decode would be returned as if it were the content"
    pub fn of_response(request_method: &[u8], head: &ResponseHead) -> Result<Self, Error> {
        // Rule 1: these never have a body, whatever their header fields say.
        // Method names are case-sensitive (RFC 9110 §9.1): `head` is some other method.
        if request_method == b"HEAD" || matches!(head.status, 100..=199 | 204 | 304) {
            return Ok(Self::None);
        }
        // Rule 2.
        if request_method == b"CONNECT" && matches!(head.status, 200..=299) {
            return Err(Error::Unsupported("a CONNECT tunnel"));
        }
        let has_transfer_encoding = head.headers.get("Transfer-Encoding").is_some();
        let has_content_length = head.headers.get("Content-Length").is_some();
        match (has_transfer_encoding, has_content_length) {
            // Rule 3.
            (true, true) => Err(Error::Framing(
                "both Transfer-Encoding and Content-Length are present",
            )),
            // Rule 4. Any other coding would need decoding this crate does not do.
            (true, false) => {
                let mut codings = head.headers.list("Transfer-Encoding");
                let chunked_alone = codings
                    .next()
                    .is_some_and(|coding| coding.eq_ignore_ascii_case(b"chunked"))
                    && codings.next().is_none();
                if chunked_alone {
                    Ok(Self::Chunked)
                } else {
                    Err(Error::Unsupported("a transfer coding other than chunked"))
                }
            }
            // Rules 5 and 6.
            (false, true) => content_length(head).map(Self::Length),
            // Rule 8.
            (false, false) => Ok(Self::UntilClose),
        }
    }
}

/// The one value every `Content-Length` list element agrees on.
fn content_length(head: &ResponseHead) -> Result<u64, Error> {
    let invalid = Error::Framing("Content-Length is not a single decimal number");
    let mut agreed = None;
    for element in head.headers.list("Content-Length") {
        // Digits only: `str::parse` would also take a leading plus sign.
        if !element.iter().all(u8::is_ascii_digit) {
            return Err(invalid);
        }
        let value = element.iter().try_fold(0_u64, |value, digit| {
            value.checked_mul(10)?.checked_add(u64::from(digit - b'0'))
        });
        let value = value.ok_or(Error::Framing("Content-Length overflows"))?;
        if agreed.is_some_and(|agreed| agreed != value) {
            return Err(invalid);
        }
        agreed = Some(value);
    }
    agreed.ok_or(invalid)
}

#[cfg(test)]
mod unit {
    use super::*;

    /// The decoder skips interim responses before it asks about framing, so this arm of
    /// rule 1 is only reachable here.
    #[test]
    fn interim_response_has_no_body_whatever_its_fields_say() {
        let raw = b"HTTP/1.1 102 Processing\r\nContent-Length: 5\r\n\r\n";
        let head = ResponseHead::parse(raw, 16).unwrap();
        assert_eq!(
            BodyFraming::of_response(b"GET", &head),
            Ok(BodyFraming::None)
        );
    }
}
