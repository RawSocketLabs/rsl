//! netascii line-ending translation (RFC 1350 §1, §5).
//!
//! TFTP's `netascii` mode carries text in a canonical form: a newline is the
//! two-byte sequence CR-LF, and a carriage return that is *not* part of a
//! newline is the two-byte sequence CR-NUL. The host translates to and from its
//! own text format on the way out and in.
//!
//! This crate treats the host format as Unix text (newline = LF, `0x0A`), which
//! is the convention on the platforms it targets. [`to_netascii`] and
//! [`from_netascii`] are inverses for any host text that uses LF newlines.
//! `octet` mode does no translation and should pass bytes through verbatim.

/// Carriage return.
const CR: u8 = b'\r';
/// Line feed.
const LF: u8 = b'\n';
/// NUL.
const NUL: u8 = 0;

/// Translates host text (LF newlines) into netascii (CR-LF newlines, with a
/// lone CR encoded as CR-NUL).
//~ implements rfc1350#5/must.b29cb3 part="encode host text to netascii"
pub fn to_netascii(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() + input.len() / 16 + 1);
    for &byte in input {
        match byte {
            LF => {
                out.push(CR);
                out.push(LF);
            }
            CR => {
                out.push(CR);
                out.push(NUL);
            }
            other => out.push(other),
        }
    }
    out
}

/// Translates netascii (CR-LF newlines, CR-NUL for a literal CR) back into host
/// text (LF newlines). A CR followed by anything other than LF or NUL is passed
/// through as a bare CR (lenient decoding).
//~ implements rfc1350#5/must.e62ee0 part="decode netascii to host text"
pub fn from_netascii(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut iter = input.iter().copied().peekable();
    while let Some(byte) = iter.next() {
        if byte == CR {
            match iter.peek().copied() {
                Some(LF) => {
                    iter.next();
                    out.push(LF);
                }
                Some(NUL) => {
                    iter.next();
                    out.push(CR);
                }
                _ => out.push(CR),
            }
        } else {
            out.push(byte);
        }
    }
    out
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn newlines_become_crlf_and_back() {
        let host = b"line1\nline2\n";
        let net = to_netascii(host);
        assert_eq!(net, b"line1\r\nline2\r\n");
        assert_eq!(from_netascii(&net), host);
    }

    #[test]
    fn lone_cr_round_trips_via_cr_nul() {
        let host = b"a\rb"; // a bare CR in host data
        let net = to_netascii(host);
        assert_eq!(net, b"a\r\0b");
        assert_eq!(from_netascii(&net), host);
    }

    #[test]
    fn octet_like_text_is_unchanged_without_cr_or_lf() {
        let host = b"plain ascii, no newlines";
        assert_eq!(to_netascii(host), host);
        assert_eq!(from_netascii(host), host);
    }

    #[test]
    fn round_trip_is_identity_for_lf_text() {
        let host = b"mixed\ntext\nwith\nlines\n";
        assert_eq!(from_netascii(&to_netascii(host)), host);
    }
}
