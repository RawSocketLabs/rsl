use binrw::binrw;

/// A TFTP opcode — the 2-byte big-endian value that opens every packet
/// (RFC 1350 §5).
///
/// The six defined opcodes are named variants; any other value is preserved as
/// [`Custom`](Opcode::Custom) rather than rejected, so decoding never loses
/// information about what was on the wire.
//~ models rfc1350#5 registry="opcode"
#[repr(u16)]
#[binrw]
#[brw(big)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Opcode {
    /// 1 — Read Request (RRQ).
    #[brw(magic = 1u16)]
    Read = 1,

    /// 2 — Write Request (WRQ).
    #[brw(magic = 2u16)]
    Write = 2,

    /// 3 — Data.
    #[brw(magic = 3u16)]
    Data = 3,

    /// 4 — Acknowledgment.
    #[brw(magic = 4u16)]
    Ack = 4,

    /// 5 — Error.
    #[brw(magic = 5u16)]
    Error = 5,

    /// 6 — Option Acknowledgment (OACK, RFC 2347).
    #[brw(magic = 6u16)]
    OptionAck = 6,

    /// Any other, unassigned opcode.
    Custom(u16),
}

#[cfg(test)]
mod unit {
    use binrw::{BinRead, BinWrite, io::Cursor};

    use super::*;

    fn parse(code: u16) -> Opcode {
        Opcode::read(&mut Cursor::new(code.to_be_bytes())).expect("parses")
    }

    #[test]
    fn opcodes_map_and_round_trip() {
        assert_eq!(parse(1), Opcode::Read);
        assert_eq!(parse(2), Opcode::Write);
        assert_eq!(parse(3), Opcode::Data);
        assert_eq!(parse(4), Opcode::Ack);
        assert_eq!(parse(5), Opcode::Error);
        assert_eq!(parse(6), Opcode::OptionAck);
        for code in [0u16, 7, 99, 0xFFFF] {
            assert_eq!(parse(code), Opcode::Custom(code), "code {code}");
        }
        for code in [0u16, 1, 2, 3, 4, 5, 6, 7, 0xFFFF] {
            let mut buf = Cursor::new(Vec::new());
            parse(code).write(&mut buf).expect("writes");
            assert_eq!(
                buf.into_inner(),
                code.to_be_bytes(),
                "code {code} round-trip"
            );
        }
    }
}
