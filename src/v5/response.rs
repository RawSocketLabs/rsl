use binrw::binrw;

/// A SOCKS5 reply code (the REP field, RFC 1928 §6 Replies).
///
/// X'00'–X'08' are the assigned codes; X'09'–X'FF' are unassigned and carried
/// as `Custom`.
//~ models rfc1928#6
#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Response {
    #[brw(magic = 0x00u8)]
    Succeeded = 0x00,

    #[brw(magic = 0x01u8)]
    GeneralFailure = 0x01,

    #[brw(magic = 0x02u8)]
    ConnectionNotAllowed = 0x02,

    #[brw(magic = 0x03u8)]
    NetworkUnreachable = 0x03,

    #[brw(magic = 0x04u8)]
    HostUnreachable = 0x04,

    #[brw(magic = 0x05u8)]
    ConnectionRefused = 0x05,

    #[brw(magic = 0x06u8)]
    TtlExpired = 0x06,

    #[brw(magic = 0x07u8)]
    CommandNotSupported = 0x07,

    #[brw(magic = 0x08u8)]
    AddressTypeNotSupported = 0x08,

    Custom(u8),
}

#[cfg(test)]
mod unit {
    use binrw::{io::Cursor, BinRead, BinWrite};

    use super::*;

    fn parse(code: u8) -> Response {
        Response::read_be(&mut Cursor::new(vec![code])).expect("parses")
    }

    /// The REP enum maps the RFC 1928 §6 reply codes and round-trips all 256.
    #[test]
    fn enum_matches_reply_codes() {
        let assigned = [
            (0x00, Response::Succeeded),
            (0x01, Response::GeneralFailure),
            (0x02, Response::ConnectionNotAllowed),
            (0x03, Response::NetworkUnreachable),
            (0x04, Response::HostUnreachable),
            (0x05, Response::ConnectionRefused),
            (0x06, Response::TtlExpired),
            (0x07, Response::CommandNotSupported),
            (0x08, Response::AddressTypeNotSupported),
        ];
        for (code, expected) in assigned {
            assert_eq!(parse(code), expected, "code {code:#04x}");
        }
        // X'09'–X'FF' are unassigned -> Custom.
        for code in [0x09u8, 0x42, 0xFF] {
            assert_eq!(parse(code), Response::Custom(code), "code {code:#04x}");
        }
        for code in 0u8..=0xFF {
            let mut buf = Cursor::new(Vec::new());
            parse(code).write_be(&mut buf).expect("writes");
            assert_eq!(buf.into_inner(), vec![code], "code {code:#04x} did not round-trip");
        }
    }
}
