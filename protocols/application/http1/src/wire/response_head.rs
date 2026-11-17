//! The head of a response: status line and header fields.

// --- Internal modules ---
use super::{Headers, headers::trim_ows, request::is_token_byte};
use crate::Error;

/// The head of one response (RFC 9112 §4 and §5).
///
/// The typed fields are a view of [`raw`](Self::raw), which keeps the head exactly as the
/// server sent it. Values are preserved whatever they are: any version digits, any
/// three-digit status, any reason phrase, unknown field names. What is refused is syntax that
/// lets two parsers disagree about where a field or the head ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseHead {
    raw: Vec<u8>,
    /// The protocol version as `(major, minor)`.
    pub version: (u8, u8),
    /// The status code.
    pub status: u16,
    /// The reason phrase, which may be empty.
    pub reason: Vec<u8>,
    /// The header fields.
    pub headers: Headers,
}

impl ResponseHead {
    /// Parses a complete head: `raw` runs from the status line through the blank line that
    /// ends the header fields. Finding that blank line is the decoder's job, which is why
    /// this is not public.
    ///
    /// Fails with [`Error::Malformed`] for a head that is not HTTP/1 syntax. Lines must end
    /// in CRLF: a lone LF and a lone CR are refused, as are a line that starts with
    /// whitespace (an obsolete folded field, §5.2), whitespace between a field name and its
    /// colon (§5.1), and NUL. Fails with [`Error::TooManyHeaders`] when there are more than
    /// `max_headers` fields.
    //~ rejects rfc9112#2.2 reason="a lone LF may be accepted as a line end; refusing it leaves one reading of where a line ends"
    //~ rejects rfc9112#5.2 reason="a folded field in a response; a user agent may instead replace the fold with spaces"
    pub(crate) fn parse(raw: &[u8], max_headers: usize) -> Result<Self, Error> {
        // Everything before the blank line: one or more lines, each ending in CRLF.
        let lines = raw
            .strip_suffix(b"\r\n")
            .filter(|lines| lines.ends_with(b"\r\n"))
            .ok_or(Error::Malformed("the head does not end in a blank line"))?;
        let mut lines = lines.split_inclusive(|&byte| byte == b'\n').map(|line| {
            line.strip_suffix(b"\r\n")
                .filter(|line| !line.contains(&b'\r'))
                .ok_or(Error::Malformed("a line does not end in exactly CRLF"))
        });
        let status_line = lines
            .next()
            .ok_or(Error::Malformed("the status line is missing"))??;
        let StatusLine {
            version,
            status,
            reason,
        } = parse_status_line(status_line)?;

        let mut headers = Headers::new();
        for line in lines {
            if headers.fields.len() == max_headers {
                return Err(Error::TooManyHeaders);
            }
            let (name, value) = parse_field_line(line?)?;
            headers.push(name, value);
        }
        Ok(Self {
            raw: raw.to_vec(),
            version,
            status,
            reason: reason.to_vec(),
            headers,
        })
    }

    /// The head exactly as received.
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
}

/// The parts of a status line.
struct StatusLine<'a> {
    version: (u8, u8),
    status: u16,
    reason: &'a [u8],
}

/// Splits `HTTP/1.1 200 OK` into its version, status code and reason phrase.
fn parse_status_line(line: &[u8]) -> Result<StatusLine<'_>, Error> {
    let malformed = Error::Malformed("the status line is not `HTTP/x.y nnn reason`");
    let [
        b'H',
        b'T',
        b'T',
        b'P',
        b'/',
        major @ b'0'..=b'9',
        b'.',
        minor @ b'0'..=b'9',
        b' ',
        a @ b'0'..=b'9',
        b @ b'0'..=b'9',
        c @ b'0'..=b'9',
        rest @ ..,
    ] = line
    else {
        return Err(malformed);
    };
    // The space after the code is required, but servers that send no reason often omit it.
    let reason = match rest {
        [] => rest,
        [b' ', reason @ ..] => reason,
        _ => return Err(malformed),
    };
    if reason.contains(&0) {
        return Err(malformed);
    }
    let digit = |byte: &u8| u16::from(byte - b'0');
    let status = digit(a) * 100 + digit(b) * 10 + digit(c);
    Ok(StatusLine {
        version: (major - b'0', minor - b'0'),
        status,
        reason,
    })
}

/// Splits `Name: value` into its name and its value without surrounding whitespace.
fn parse_field_line(line: &[u8]) -> Result<(&[u8], &[u8]), Error> {
    if matches!(line.first(), Some(b' ' | b'\t')) {
        return Err(Error::Malformed("a header line starts with whitespace"));
    }
    let colon = line
        .iter()
        .position(|&byte| byte == b':')
        .ok_or(Error::Malformed("a header line has no colon"))?;
    let (name, value) = (&line[..colon], &line[colon + 1..]);
    if name.is_empty() || !name.iter().all(|&byte| is_token_byte(byte)) {
        return Err(Error::Malformed("a header name is not a token"));
    }
    if value.contains(&0) {
        return Err(Error::Malformed("a header value contains NUL"));
    }
    Ok((name, trim_ows(value)))
}
