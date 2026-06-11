use binrw::binrw;

/// A SOCKS4 request command code (the CD field of a request).
///
/// SOCKS4 defines exactly two commands — CONNECT and BIND. Any other code is
/// kept verbatim as [`Custom`](Command::Custom) rather than rejected, so the
/// parser never loses information about what was on the wire (a server answers
/// an unknown command with [`ReplyCode::Rejected`](crate::v4::ReplyCode)).
//~ models socks4#front registry="CD-request"
//~ implements socks4a#front/should.9ced49 part="CONNECT=1 / BIND=2 codes"
#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// 1 — CONNECT: open a TCP connection to the destination.
    #[brw(magic = 1u8)]
    Connect = 1,

    /// 2 — BIND: listen for an inbound connection from the destination, used
    /// by protocols (e.g. active-mode FTP) where the peer dials back.
    #[brw(magic = 2u8)]
    Bind = 2,

    /// Any other, unassigned command byte.
    Custom(u8),
}

#[cfg(test)]
mod unit {
    use binrw::{io::Cursor, BinRead, BinWrite};

    use super::*;

    fn parse(code: u8) -> Command {
        Command::read_be(&mut Cursor::new(vec![code])).expect("parses")
    }

    //~ verifies socks4#front/should.01af87
    //~ verifies socks4#front/must.6e79ac
    #[test]
    fn enum_matches_command_codes_and_round_trips_all_256() {
        assert_eq!(parse(1), Command::Connect);
        assert_eq!(parse(2), Command::Bind);
        for code in [0u8, 3, 0x7F, 0xFF] {
            assert_eq!(parse(code), Command::Custom(code), "code {code:#04x}");
        }
        for code in 0u8..=0xFF {
            let mut buf = Cursor::new(Vec::new());
            parse(code).write_be(&mut buf).expect("writes");
            assert_eq!(buf.into_inner(), vec![code], "code {code:#04x} did not round-trip");
        }
    }
}
