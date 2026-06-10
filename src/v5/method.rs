use binrw::binrw;

/// A SOCKS5 authentication method code (RFC 1928 §3 method-number registry).
///
/// The three assigned methods and the no-acceptable-methods sentinel are named
/// variants; every other code — the IANA-assigned range X'03'–X'7F' and the
/// private range X'80'–X'FE' — is an opaque [`Method::Custom`]. We do not
/// distinguish those ranges because the distinction is administrative, not
/// behavioral: a code is either one we hold an authenticator for, or it is not.
//~ implements rfc1928#3/required.ce9732
#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Method {
    /// X'00' NO AUTHENTICATION REQUIRED.
    #[brw(magic = 0x00u8)]
    NoAuth = 0x00,

    /// X'01' GSSAPI.
    #[brw(magic = 0x01u8)]
    GssApi = 0x01,

    /// X'02' USERNAME/PASSWORD.
    #[brw(magic = 0x02u8)]
    Plain = 0x02,

    /// X'FF' NO ACCEPTABLE METHODS.
    #[brw(magic = 0xFFu8)]
    NoAcceptableMethods = 0xFF,

    /// Any other code (X'03'–X'7F' IANA assigned, X'80'–X'FE' private).
    /// Discriminant is arbitrary (binrw matches by magic); 0x04 keeps it
    /// distinct without overflowing repr(u8).
    Custom(u8) = 0x04,
}

#[cfg(test)]
mod unit {
    use binrw::{io::Cursor, BinRead, BinWrite};

    use super::*;

    fn parse(code: u8) -> Method {
        Method::read_be(&mut Cursor::new(vec![code])).expect("parses")
    }

    #[test]
    fn test_no_acceptable_methods_parses() {
        assert_eq!(parse(0xFF), Method::NoAcceptableMethods);
    }

    #[test]
    fn test_custom_parses() {
        assert_eq!(parse(0x42), Method::Custom(0x42));
    }

    /// Verifies the enum faithfully models the RFC 1928 §3 method-number
    /// registry: the assigned codes map to their named variants, the
    /// IANA-assigned and private ranges map to `Custom`, and every one of the
    /// 256 codes round-trips back to its byte.
    #[test]
    fn enum_matches_method_number_registry() {
        // Assigned codes and the sentinel.
        assert_eq!(parse(0x00), Method::NoAuth);
        assert_eq!(parse(0x01), Method::GssApi);
        assert_eq!(parse(0x02), Method::Plain);
        assert_eq!(parse(0xFF), Method::NoAcceptableMethods);

        // Boundaries of the IANA-assigned (X'03'–X'7F') and private
        // (X'80'–X'FE') ranges are opaque Custom codes.
        for code in [0x03u8, 0x04, 0x7F, 0x80, 0xFE] {
            assert_eq!(parse(code), Method::Custom(code), "code {code:#04x}");
        }

        // No code is lost or remapped: all 256 values round-trip.
        for code in 0u8..=0xFF {
            let mut buf = Cursor::new(Vec::new());
            parse(code).write_be(&mut buf).expect("writes");
            assert_eq!(buf.into_inner(), vec![code], "code {code:#04x} did not round-trip");
        }
    }
}
