use crate::error::{Result, TftpError};
use crate::wire::codec::{ByteReader, put_cstr};
use crate::wire::option::TftpOption;

/// An Option Acknowledgment (OACK, RFC 2347): opcode 6 followed by the
/// `name\0value\0` pairs the server has accepted.
///
/// ```text
/// 2 bytes   [ string  1b  string  1b ]...
/// ---------------------------------------
/// | 0x0006 |  [ opt-N | 0 | val-N | 0 ]...
/// ---------------------------------------
/// ```
///
/// The server must include only options the client actually requested
/// (RFC 2347); this type simply carries whatever set it is given, leaving that
/// policy to the high-level server.
//~ models rfc2347#front
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct OptionAck {
    /// The acknowledged options.
    pub options: Vec<TftpOption>,
}

impl OptionAck {
    /// Creates an OACK carrying `options`.
    pub fn new(options: Vec<TftpOption>) -> Self {
        Self { options }
    }

    /// Decodes an OACK from a complete datagram (including the opcode).
    ///
    /// # Errors
    /// Returns [`TftpError::Decode`] if the opcode is not 6 or a string field
    /// is unterminated.
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut reader = ByteReader::new(buf);
        let opcode = reader.read_u16()?;
        if opcode != 6 {
            return Err(TftpError::Decode(format!("opcode {opcode} is not OACK")));
        }
        let mut options = Vec::new();
        while !reader.is_empty() {
            let name = reader.read_cstr()?;
            let value = reader.read_cstr()?;
            options.push(TftpOption::from_pair(
                &String::from_utf8_lossy(&name),
                &String::from_utf8_lossy(&value),
            ));
        }
        Ok(Self { options })
    }

    /// Encodes the OACK to its wire bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2 + self.options.len() * 12);
        out.extend_from_slice(&6u16.to_be_bytes());
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
    fn oack_round_trips() {
        let oack = OptionAck::new(vec![TftpOption::BlkSize(1432), TftpOption::Timeout(5)]);
        let bytes = oack.encode();
        assert_eq!(&bytes[..2], &[0x00, 0x06]);
        assert_eq!(&bytes[2..], b"blksize\x001432\x00timeout\x005\x00");
        assert_eq!(OptionAck::decode(&bytes).unwrap(), oack);
    }

    #[test]
    fn rejects_wrong_opcode() {
        assert!(OptionAck::decode(b"\x00\x04\x00\x00").is_err());
    }
}
