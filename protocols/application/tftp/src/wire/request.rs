use crate::error::{Result, TftpError};
use crate::wire::codec::{ByteReader, put_cstr};
use crate::wire::mode::TransferMode;
#[cfg(feature = "options")]
use crate::wire::option::TftpOption;

/// Whether a request reads from (RRQ) or writes to (WRQ) the server.
//~ models rfc1350#5 registry="request-kind"
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RequestKind {
    /// RRQ — opcode 1; the client downloads a file.
    Read,
    /// WRQ — opcode 2; the client uploads a file.
    Write,
}

impl RequestKind {
    /// The opcode value for this request kind.
    pub fn opcode(self) -> u16 {
        match self {
            RequestKind::Read => 1,
            RequestKind::Write => 2,
        }
    }

    /// Maps an opcode value to a request kind, if it is 1 or 2.
    pub fn from_opcode(opcode: u16) -> Option<Self> {
        match opcode {
            1 => Some(RequestKind::Read),
            2 => Some(RequestKind::Write),
            _ => None,
        }
    }
}

/// A Read or Write request (RRQ/WRQ, RFC 1350 §5; options per RFC 2347).
///
/// ```text
/// 2 bytes    string    1b    string    1b   [ string  1b  string  1b ]...
/// -----------------------------------------------------------------------
/// | Opcode | Filename | 0 |  Mode   |  0 |  [ opt-N | 0 | val-N | 0 ]...
/// -----------------------------------------------------------------------
/// ```
///
/// `filename` is stored as raw bytes (faithful to the wire and to dual-use:
/// a non-UTF-8 name is representable); use [`filename_lossy`](Self::filename_lossy)
/// for a string view. The trailing option pairs exist only under the `options`
/// feature; without it they are ignored on decode.
//~ models rfc1350#5
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// Read or write.
    pub kind: RequestKind,
    /// The filename (or, for `mail` mode, the recipient), raw wire bytes.
    pub filename: Vec<u8>,
    /// The transfer mode.
    pub mode: TransferMode,
    /// Negotiated options (RFC 2347), in request order.
    #[cfg(feature = "options")]
    pub options: Vec<TftpOption>,
}

impl Request {
    /// Builds a Read request for `filename` in `mode`.
    pub fn read(filename: impl Into<Vec<u8>>, mode: TransferMode) -> Self {
        Self::new(RequestKind::Read, filename, mode)
    }

    /// Builds a Write request for `filename` in `mode`.
    pub fn write(filename: impl Into<Vec<u8>>, mode: TransferMode) -> Self {
        Self::new(RequestKind::Write, filename, mode)
    }

    fn new(kind: RequestKind, filename: impl Into<Vec<u8>>, mode: TransferMode) -> Self {
        Self {
            kind,
            filename: filename.into(),
            mode,
            #[cfg(feature = "options")]
            options: Vec::new(),
        }
    }

    /// Attaches negotiation options to the request (builder-style).
    #[cfg(feature = "options")]
    pub fn with_options(mut self, options: Vec<TftpOption>) -> Self {
        self.options = options;
        self
    }

    /// The filename as a lossy UTF-8 string.
    pub fn filename_lossy(&self) -> String {
        String::from_utf8_lossy(&self.filename).into_owned()
    }

    /// Decodes a request from a complete datagram (including the opcode).
    ///
    /// # Errors
    /// Returns [`TftpError::Decode`] if the opcode is not RRQ/WRQ or a string
    /// field is unterminated.
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut reader = ByteReader::new(buf);
        let opcode = reader.read_u16()?;
        let kind = RequestKind::from_opcode(opcode)
            .ok_or_else(|| TftpError::Decode(format!("opcode {opcode} is not RRQ/WRQ")))?;
        let filename = reader.read_cstr()?;
        let mode = TransferMode::from_bytes(&reader.read_cstr()?);

        #[cfg(feature = "options")]
        let options = {
            let mut options = Vec::new();
            //~ implements rfc2347#front/may.4e00b2 part="parse trailing option pairs"
            while !reader.is_empty() {
                let name = reader.read_cstr()?;
                let value = reader.read_cstr()?;
                options.push(TftpOption::from_pair(
                    &String::from_utf8_lossy(&name),
                    &String::from_utf8_lossy(&value),
                ));
            }
            options
        };

        Ok(Self {
            kind,
            filename,
            mode,
            #[cfg(feature = "options")]
            options,
        })
    }

    /// Encodes the request to its wire bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + self.filename.len() + self.mode.as_str().len());
        out.extend_from_slice(&self.kind.opcode().to_be_bytes());
        put_cstr(&mut out, &self.filename);
        put_cstr(&mut out, self.mode.as_str().as_bytes());
        #[cfg(feature = "options")]
        for opt in &self.options {
            put_cstr(&mut out, opt.name().as_bytes());
            put_cstr(&mut out, opt.value().as_bytes());
        }
        out
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn rrq_round_trips() {
        let req = Request::read("boot.img", TransferMode::Octet);
        let bytes = req.encode();
        assert_eq!(&bytes[..2], &[0x00, 0x01]);
        assert_eq!(&bytes[2..], b"boot.img\0octet\0");
        assert_eq!(Request::decode(&bytes).unwrap(), req);
    }

    #[test]
    fn wrq_round_trips_with_mode_casing_normalized() {
        // An uppercase mode on the wire decodes to the canonical lower-case.
        let bytes = b"\x00\x02file\0NETASCII\0";
        let req = Request::decode(bytes).unwrap();
        assert_eq!(req.kind, RequestKind::Write);
        assert_eq!(req.mode, TransferMode::NetAscii);
        assert_eq!(req.filename_lossy(), "file");
    }

    #[test]
    fn rejects_non_request_opcode() {
        assert!(Request::decode(b"\x00\x03file\0octet\0").is_err());
    }

    #[cfg(feature = "options")]
    #[test]
    fn rrq_round_trips_with_options() {
        let req = Request::read("big.iso", TransferMode::Octet)
            .with_options(vec![TftpOption::BlkSize(1432), TftpOption::TSize(0)]);
        let bytes = req.encode();
        assert!(bytes.ends_with(b"blksize\x001432\x00tsize\x000\x00"));
        assert_eq!(Request::decode(&bytes).unwrap(), req);
    }
}
