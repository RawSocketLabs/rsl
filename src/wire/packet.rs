use crate::error::{Result, TftpError};
use crate::wire::ack::Ack;
use crate::wire::data::Data;
use crate::wire::error_packet::ErrorPacket;
use crate::wire::opcode::Opcode;
use crate::wire::request::Request;
#[cfg(feature = "options")]
use crate::wire::oack::OptionAck;

/// Any TFTP packet, tagged by opcode — the unifying type the transport reads
/// and writes.
///
/// Decode a whole datagram with [`Packet::decode`] and serialize with
/// [`Packet::encode`]; or use the per-type `decode`/`encode` when you already
/// know the shape.
//~ models rfc1350#5
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Packet {
    /// A Read or Write request (RRQ/WRQ).
    Request(Request),
    /// A DATA block.
    Data(Data),
    /// An ACK.
    Ack(Ack),
    /// An ERROR.
    Error(ErrorPacket),
    /// An Option Acknowledgment (RFC 2347).
    #[cfg(feature = "options")]
    OptionAck(OptionAck),
}

impl Packet {
    /// The opcode of this packet.
    pub fn opcode(&self) -> Opcode {
        match self {
            Packet::Request(req) => match req.kind {
                crate::wire::request::RequestKind::Read => Opcode::Read,
                crate::wire::request::RequestKind::Write => Opcode::Write,
            },
            Packet::Data(_) => Opcode::Data,
            Packet::Ack(_) => Opcode::Ack,
            Packet::Error(_) => Opcode::Error,
            #[cfg(feature = "options")]
            Packet::OptionAck(_) => Opcode::OptionAck,
        }
    }

    /// Decodes a complete datagram into a typed packet.
    ///
    /// # Errors
    /// Returns [`TftpError::Decode`] for a truncated datagram, an opcode with no
    /// typed representation (e.g. OACK when the `options` feature is off), or a
    /// malformed body.
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let opcode = buf
            .get(0..2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .ok_or_else(|| TftpError::Decode("datagram shorter than an opcode".into()))?;
        match opcode {
            1 | 2 => Ok(Packet::Request(Request::decode(buf)?)),
            3 => Ok(Packet::Data(Data::decode(buf)?)),
            4 => Ok(Packet::Ack(Ack::decode(buf)?)),
            5 => Ok(Packet::Error(ErrorPacket::decode(buf)?)),
            #[cfg(feature = "options")]
            6 => Ok(Packet::OptionAck(OptionAck::decode(buf)?)),
            #[cfg(not(feature = "options"))]
            6 => Err(TftpError::Decode(
                "OACK received but the `options` feature is disabled".into(),
            )),
            other => Err(TftpError::Decode(format!("unknown opcode {other}"))),
        }
    }

    /// Encodes the packet to its wire bytes.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Packet::Request(req) => req.encode(),
            Packet::Data(data) => data.encode(),
            Packet::Ack(ack) => ack.encode(),
            Packet::Error(err) => err.encode(),
            #[cfg(feature = "options")]
            Packet::OptionAck(oack) => oack.encode(),
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::wire::error_code::ErrorCode;
    use crate::wire::mode::TransferMode;

    #[test]
    fn dispatches_each_opcode() {
        let cases = [
            Packet::Request(Request::read("f", TransferMode::Octet)),
            Packet::Data(Data::new(1, b"x".to_vec())),
            Packet::Ack(Ack::new(1)),
            Packet::Error(ErrorPacket::new(ErrorCode::FileNotFound, "nope")),
        ];
        for packet in cases {
            let round = Packet::decode(&packet.encode()).unwrap();
            assert_eq!(round, packet);
        }
    }

    #[test]
    fn unknown_opcode_is_rejected() {
        assert!(Packet::decode(&[0x00, 0x63, 0x00]).is_err());
        assert!(Packet::decode(&[]).is_err());
    }
}
