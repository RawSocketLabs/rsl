use binrw::{binrw, io::Cursor, BinRead, BinWrite, NullString};

use crate::error::Result;
use crate::wire::error_code::ErrorCode;

/// An ERROR packet (RFC 1350 §5): opcode 5, a 2-byte [`ErrorCode`], then a
/// NUL-terminated, human-readable netascii message. An ERROR is a courtesy
/// notification — it is not acknowledged and does not, by itself, end a
/// transfer that is already over.
///
/// ```text
/// 2 bytes   2 bytes        string    1 byte
/// -----------------------------------------
/// | 0x0005 | ErrorCode |   ErrMsg   |  0   |
/// -----------------------------------------
/// ```
//~ models rfc1350#5
//~ partial rfc1350#5/should.166d37 missing="message carried verbatim; no netascii translation applied to it"
#[binrw]
#[brw(big, magic = 5u16)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorPacket {
    /// The error code.
    pub code: ErrorCode,
    /// The human-readable message (netascii on the wire), NUL-terminated.
    pub message: NullString,
}

impl ErrorPacket {
    /// Creates an ERROR packet with `code` and `message`.
    pub fn new(code: ErrorCode, message: impl Into<NullString>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The message as a lossy UTF-8 string.
    pub fn message_lossy(&self) -> String {
        String::from_utf8_lossy(&self.message).into_owned()
    }

    /// Decodes an ERROR packet from a complete datagram (including the opcode).
    ///
    /// # Errors
    /// Returns [`crate::error::TftpError::Decode`] if the bytes are not a valid
    /// ERROR packet.
    pub fn decode(buf: &[u8]) -> Result<Self> {
        Ok(Self::read(&mut Cursor::new(buf))?)
    }

    /// Encodes the ERROR packet to its wire bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::with_capacity(5 + self.message.len()));
        self.write(&mut cursor).expect("writing to a Vec cannot fail");
        cursor.into_inner()
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn error_round_trips() {
        let err = ErrorPacket::new(ErrorCode::FileNotFound, "File not found");
        let bytes = err.encode();
        assert_eq!(&bytes[..4], &[0x00, 0x05, 0x00, 0x01]);
        assert_eq!(&bytes[4..], b"File not found\0");
        assert_eq!(ErrorPacket::decode(&bytes).unwrap(), err);
        assert_eq!(err.message_lossy(), "File not found");
    }
}
