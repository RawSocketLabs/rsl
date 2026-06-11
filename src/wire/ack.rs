use binrw::{binrw, io::Cursor, BinRead, BinWrite};

use crate::error::Result;

/// An ACK packet (RFC 1350 §5): opcode 4 followed by the 2-byte block number
/// being acknowledged. The block-0 ACK is also the server's response to a WRQ.
///
/// ```text
/// 2 bytes   2 bytes
/// -------------------
/// | 0x0004 | Block # |
/// -------------------
/// ```
//~ models rfc1350#5
#[binrw]
#[brw(big, magic = 4u16)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Ack {
    /// The block number being acknowledged.
    pub block: u16,
}

impl Ack {
    /// Creates an ACK for `block`.
    pub fn new(block: u16) -> Self {
        Self { block }
    }

    /// Decodes an ACK from a complete datagram (including the opcode).
    ///
    /// # Errors
    /// Returns [`crate::error::TftpError::Decode`] if the bytes are not a valid
    /// ACK packet.
    pub fn decode(buf: &[u8]) -> Result<Self> {
        Ok(Self::read(&mut Cursor::new(buf))?)
    }

    /// Encodes the ACK to its wire bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::with_capacity(4));
        self.write(&mut cursor).expect("writing to a Vec cannot fail");
        cursor.into_inner()
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn ack_round_trips() {
        let ack = Ack::new(0x1234);
        assert_eq!(ack.encode(), vec![0x00, 0x04, 0x12, 0x34]);
        assert_eq!(Ack::decode(&ack.encode()).unwrap(), ack);
    }

    #[test]
    fn rejects_wrong_opcode() {
        assert!(Ack::decode(&[0x00, 0x03, 0x00, 0x01]).is_err());
    }
}
