//! Minimal byte-cursor helpers for the hand-decoded packets (Request, OACK),
//! whose layout is a sequence of NUL-terminated strings rather than a fixed
//! binary shape.
//!
//! TFTP runs over UDP, so a packet is always a single complete datagram — there
//! is no streaming-framing problem. Decoders parse a whole buffer; these helpers
//! make that liberal and bounds-checked.

use crate::error::{Result, TftpError};

/// A read cursor over a complete datagram.
pub(crate) struct ByteReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Reads a big-endian `u16`, or errors if fewer than two bytes remain.
    pub(crate) fn read_u16(&mut self) -> Result<u16> {
        let end = self.pos + 2;
        let bytes = self
            .buf
            .get(self.pos..end)
            .ok_or_else(|| TftpError::Decode("truncated: expected a 2-byte field".into()))?;
        self.pos = end;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Reads bytes up to and consuming the next NUL terminator, returning the
    /// content without the NUL. Errors if no terminator is present (an
    /// unterminated string is physically unparseable).
    pub(crate) fn read_cstr(&mut self) -> Result<Vec<u8>> {
        let rest = &self.buf[self.pos..];
        match rest.iter().position(|&b| b == 0) {
            Some(nul) => {
                let out = rest[..nul].to_vec();
                self.pos += nul + 1;
                Ok(out)
            }
            None => Err(TftpError::Decode(
                "truncated: unterminated NUL-terminated string".into(),
            )),
        }
    }

    /// Whether the cursor has consumed the whole buffer. Used to drive
    /// option-pair parsing, so it is only exercised with the `options` feature.
    #[cfg_attr(not(feature = "options"), allow(dead_code))]
    pub(crate) fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }
}

/// Appends a NUL-terminated string field to an encode buffer.
pub(crate) fn put_cstr(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(bytes);
    out.push(0);
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn reads_fields_and_detects_truncation() {
        let mut r = ByteReader::new(b"\x00\x01file\0octet\0");
        assert_eq!(r.read_u16().unwrap(), 1);
        assert_eq!(r.read_cstr().unwrap(), b"file");
        assert_eq!(r.read_cstr().unwrap(), b"octet");
        assert!(r.is_empty());

        assert!(ByteReader::new(b"\x00").read_u16().is_err());
        assert!(ByteReader::new(b"no-nul").read_cstr().is_err());
    }
}
