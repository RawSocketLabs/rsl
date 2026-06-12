use binrw::binrw;

/// A SOCKS5 request command code (the CMD field, RFC 1928 §4 Requests).
///
/// The three defined commands are named variants; any other code is an opaque
/// `Custom`, which the server answers with `CommandNotSupported`.
//~ models rfc1928#4 registry="COMMAND"
#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Command {
    /// X'01' CONNECT.
    #[brw(magic = 0x01u8)]
    Connect = 0x01,

    /// X'02' BIND.
    #[brw(magic = 0x02u8)]
    Bind = 0x02,

    /// X'03' UDP ASSOCIATE.
    #[brw(magic = 0x03u8)]
    UdpAssociate = 0x03,

    /// Any unassigned command code.
    Custom(u8),
}

#[cfg(test)]
mod unit {
    use binrw::{BinRead, BinWrite, io::Cursor};

    use super::*;

    fn parse(code: u8) -> Command {
        Command::read_be(&mut Cursor::new(vec![code])).expect("parses")
    }

    /// The CMD enum maps the RFC 1928 §4 command codes and round-trips all 256.
    #[test]
    fn enum_matches_command_codes() {
        assert_eq!(parse(0x01), Command::Connect);
        assert_eq!(parse(0x02), Command::Bind);
        assert_eq!(parse(0x03), Command::UdpAssociate);
        for code in [0x00u8, 0x04, 0x7F, 0xFF] {
            assert_eq!(parse(code), Command::Custom(code), "code {code:#04x}");
        }
        for code in 0u8..=0xFF {
            let mut buf = Cursor::new(Vec::new());
            parse(code).write_be(&mut buf).expect("writes");
            assert_eq!(
                buf.into_inner(),
                vec![code],
                "code {code:#04x} did not round-trip"
            );
        }
    }
}
