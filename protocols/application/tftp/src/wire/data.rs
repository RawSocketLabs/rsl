use binrw::{BinRead, BinWrite, binrw, io::Cursor};

use crate::error::Result;

/// A DATA packet (RFC 1350 §5): opcode 3, a 2-byte block number, then the block
/// payload. A payload shorter than the negotiated block size (512 by default)
/// marks the final block and ends the transfer.
///
/// ```text
/// 2 bytes   2 bytes   n bytes
/// ----------------------------
/// | 0x0003 | Block # |  Data  |
/// ----------------------------
/// ```
///
/// The payload is modeled as an arbitrary byte vector — the codec imposes no
/// length limit, so an over-long (non-conformant) block is representable for
/// dual-use; the high-level client/server enforce the negotiated size.
//~ models rfc1350#5
#[binrw]
#[brw(big, magic = 3u16)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Data {
    /// The block number, starting at 1 and incrementing (wrapping at 65535).
    pub block: u16,
    /// The block payload.
    #[br(parse_with = binrw::helpers::until_eof)]
    pub payload: Vec<u8>,
}

impl Data {
    /// Creates a DATA packet for `block` carrying `payload`.
    pub fn new(block: u16, payload: impl Into<Vec<u8>>) -> Self {
        Self {
            block,
            payload: payload.into(),
        }
    }

    /// Decodes a DATA packet from a complete datagram (including the opcode).
    ///
    /// # Errors
    /// Returns [`crate::error::TftpError::Decode`] if the bytes are not a valid
    /// DATA packet.
    pub fn decode(buf: &[u8]) -> Result<Self> {
        Ok(Self::read(&mut Cursor::new(buf))?)
    }

    /// Encodes the DATA packet to its wire bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::with_capacity(4 + self.payload.len()));
        self.write(&mut cursor)
            .expect("writing to a Vec cannot fail");
        cursor.into_inner()
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn data_round_trips_with_payload() {
        let data = Data::new(1, b"hello".to_vec());
        assert_eq!(
            data.encode(),
            [&[0x00, 0x03, 0x00, 0x01][..], b"hello"].concat()
        );
        assert_eq!(Data::decode(&data.encode()).unwrap(), data);
    }

    #[test]
    fn empty_final_block_round_trips() {
        // A zero-length payload is the canonical "exactly a multiple of the
        // block size" terminator.
        let data = Data::new(7, Vec::new());
        assert_eq!(data.encode(), vec![0x00, 0x03, 0x00, 0x07]);
        let decoded = Data::decode(&data.encode()).unwrap();
        assert!(decoded.payload.is_empty());
        assert_eq!(decoded.block, 7);
    }
}
