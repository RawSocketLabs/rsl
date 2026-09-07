use binrw::binrw;

/// A SOCKS4 reply result code (the CD field of a reply).
///
/// SOCKS4 numbers its result codes from 90. `90` is the only success; the
/// others report distinct refusals. Any unassigned byte is preserved as
/// [`Custom`](ReplyCode::Custom).
//~ models socks4#front registry="CD-reply"
#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ReplyCode {
    /// 90 — request granted.
    #[brw(magic = 90u8)]
    Granted = 90,

    /// 91 — request rejected or failed.
    #[brw(magic = 91u8)]
    Rejected = 91,

    /// 92 — request rejected because SOCKS server cannot connect to identd on
    /// the client.
    #[brw(magic = 92u8)]
    IdentdUnreachable = 92,

    /// 93 — request rejected because the client program and identd report
    /// different user-ids.
    #[brw(magic = 93u8)]
    IdentdMismatch = 93,

    /// Any other, unassigned reply byte.
    Custom(u8),
}

impl ReplyCode {
    /// Whether this code reports success (only `90`, request granted).
    pub fn is_granted(self) -> bool {
        self == ReplyCode::Granted
    }
}

#[cfg(test)]
mod unit {
    use binrw::{BinRead, BinWrite, io::Cursor};

    use super::*;

    fn parse(code: u8) -> ReplyCode {
        ReplyCode::read_be(&mut Cursor::new(vec![code])).expect("parses")
    }

    #[test]
    fn enum_matches_reply_codes_and_round_trips_all_256() {
        assert_eq!(parse(90), ReplyCode::Granted);
        assert_eq!(parse(91), ReplyCode::Rejected);
        assert_eq!(parse(92), ReplyCode::IdentdUnreachable);
        assert_eq!(parse(93), ReplyCode::IdentdMismatch);
        assert!(ReplyCode::Granted.is_granted());
        assert!(!ReplyCode::Rejected.is_granted());
        for code in [0u8, 89, 94, 0xFF] {
            assert_eq!(parse(code), ReplyCode::Custom(code), "code {code:#04x}");
            assert!(!ReplyCode::Custom(code).is_granted());
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
