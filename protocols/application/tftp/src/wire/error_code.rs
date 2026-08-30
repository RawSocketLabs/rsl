use binrw::binrw;

/// A TFTP error code — the 2-byte value carried in an ERROR packet (RFC 1350
/// §5, Appendix; code 8 added by RFC 2347).
///
/// Codes 0–8 are the assigned values; any other is preserved as
/// [`Custom`](ErrorCode::Custom).
//~ models rfc1350#5 registry="error-code"
//~ implements rfc2347#front/should.937df4 part="error code 8 for option negotiation"
#[repr(u16)]
#[binrw]
#[brw(big)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// 0 — Not defined; see the accompanying message.
    #[brw(magic = 0u16)]
    NotDefined = 0,

    /// 1 — File not found.
    #[brw(magic = 1u16)]
    FileNotFound = 1,

    /// 2 — Access violation.
    #[brw(magic = 2u16)]
    AccessViolation = 2,

    /// 3 — Disk full or allocation exceeded.
    #[brw(magic = 3u16)]
    DiskFull = 3,

    /// 4 — Illegal TFTP operation.
    #[brw(magic = 4u16)]
    IllegalOperation = 4,

    /// 5 — Unknown transfer ID.
    #[brw(magic = 5u16)]
    UnknownTransferId = 5,

    /// 6 — File already exists.
    #[brw(magic = 6u16)]
    FileAlreadyExists = 6,

    /// 7 — No such user.
    #[brw(magic = 7u16)]
    NoSuchUser = 7,

    /// 8 — Option negotiation failed / transfer terminated (RFC 2347).
    #[brw(magic = 8u16)]
    OptionNegotiation = 8,

    /// Any other, unassigned error code.
    Custom(u16),
}

#[cfg(test)]
mod unit {
    use binrw::{BinRead, BinWrite, io::Cursor};

    use super::*;

    fn parse(code: u16) -> ErrorCode {
        ErrorCode::read(&mut Cursor::new(code.to_be_bytes())).expect("parses")
    }

    #[test]
    fn error_codes_map_and_round_trip() {
        let assigned = [
            (0, ErrorCode::NotDefined),
            (1, ErrorCode::FileNotFound),
            (2, ErrorCode::AccessViolation),
            (3, ErrorCode::DiskFull),
            (4, ErrorCode::IllegalOperation),
            (5, ErrorCode::UnknownTransferId),
            (6, ErrorCode::FileAlreadyExists),
            (7, ErrorCode::NoSuchUser),
            (8, ErrorCode::OptionNegotiation),
        ];
        for (code, expected) in assigned {
            assert_eq!(parse(code), expected, "code {code}");
        }
        for code in [9u16, 42, 0xFFFF] {
            assert_eq!(parse(code), ErrorCode::Custom(code), "code {code}");
        }
        for code in [0u16, 8, 9, 0xFFFF] {
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
