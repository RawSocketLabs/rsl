use bnb::BitEnum;

/// A SOCKS5 authentication method number (RFC 1928 §3).
//~ models rfc1928#3 registry="METHOD"
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum AuthMethod {
    /// No authentication required (`0x00`).
    NoAuthentication = 0x00,
    /// GSS-API (`0x01`, RFC 1961).
    GssApi = 0x01,
    /// Username/password (`0x02`, RFC 1929).
    UsernamePassword = 0x02,
    /// The server found no acceptable offered method (`0xff`).
    NoAcceptable = 0xff,
    /// Any other IANA-assigned or private-use method number, preserved verbatim.
    #[catch_all]
    Other(u8) = 0x03,
}

/// A SOCKS5 request command (RFC 1928 §4).
//~ models rfc1928#4 registry="COMMAND"
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum Command {
    /// Establish an outbound TCP connection (`0x01`).
    Connect = 0x01,
    /// Listen for an inbound TCP connection (`0x02`).
    Bind = 0x02,
    /// Establish a UDP relay association (`0x03`).
    UdpAssociate = 0x03,
    /// Any unassigned command code, preserved verbatim.
    #[catch_all]
    Other(u8),
}

/// A SOCKS5 server reply code (RFC 1928 §6).
//~ models rfc1928#6 registry="REPLY"
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum ReplyCode {
    /// Request succeeded (`0x00`).
    Succeeded = 0x00,
    /// General SOCKS server failure (`0x01`).
    GeneralFailure = 0x01,
    /// Connection not allowed by the ruleset (`0x02`).
    ConnectionNotAllowed = 0x02,
    /// Network unreachable (`0x03`).
    NetworkUnreachable = 0x03,
    /// Host unreachable (`0x04`).
    HostUnreachable = 0x04,
    /// Connection refused (`0x05`).
    ConnectionRefused = 0x05,
    /// TTL expired (`0x06`).
    TtlExpired = 0x06,
    /// Command not supported (`0x07`).
    CommandNotSupported = 0x07,
    /// Address type not supported (`0x08`).
    AddressTypeNotSupported = 0x08,
    /// Any unassigned reply code, preserved verbatim.
    #[catch_all]
    Other(u8),
}

/// A SOCKS5 address type (RFC 1928 §4).
//~ models rfc1928#4 registry="ATYP"
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum AddressType {
    /// An IPv4 address (`0x01`).
    Ipv4 = 0x01,
    /// A length-prefixed domain name (`0x03`).
    Domain = 0x03,
    /// An IPv6 address (`0x04`).
    Ipv6 = 0x04,
    /// Any unassigned address type, preserved as a standalone code.
    #[catch_all]
    Other(u8),
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn method_registry_preserves_every_code() {
        assert_eq!(AuthMethod::from(0x00), AuthMethod::NoAuthentication);
        assert_eq!(AuthMethod::from(0x01), AuthMethod::GssApi);
        assert_eq!(AuthMethod::from(0x02), AuthMethod::UsernamePassword);
        assert_eq!(AuthMethod::from(0xff), AuthMethod::NoAcceptable);
        for code in 0u8..=u8::MAX {
            assert_eq!(u8::from(AuthMethod::from(code)), code, "method {code:#04x}");
        }
    }

    #[test]
    fn command_registry_preserves_every_code() {
        assert_eq!(Command::from(0x01), Command::Connect);
        assert_eq!(Command::from(0x02), Command::Bind);
        assert_eq!(Command::from(0x03), Command::UdpAssociate);
        for code in 0u8..=u8::MAX {
            assert_eq!(u8::from(Command::from(code)), code, "command {code:#04x}");
        }
    }

    #[test]
    fn reply_registry_preserves_every_code() {
        for (code, expected) in [
            (0x00, ReplyCode::Succeeded),
            (0x01, ReplyCode::GeneralFailure),
            (0x02, ReplyCode::ConnectionNotAllowed),
            (0x03, ReplyCode::NetworkUnreachable),
            (0x04, ReplyCode::HostUnreachable),
            (0x05, ReplyCode::ConnectionRefused),
            (0x06, ReplyCode::TtlExpired),
            (0x07, ReplyCode::CommandNotSupported),
            (0x08, ReplyCode::AddressTypeNotSupported),
        ] {
            assert_eq!(ReplyCode::from(code), expected);
        }
        for code in 0u8..=u8::MAX {
            assert_eq!(u8::from(ReplyCode::from(code)), code, "reply {code:#04x}");
        }
    }

    #[test]
    fn address_registry_preserves_every_code() {
        assert_eq!(AddressType::from(0x01), AddressType::Ipv4);
        assert_eq!(AddressType::from(0x03), AddressType::Domain);
        assert_eq!(AddressType::from(0x04), AddressType::Ipv6);
        for code in 0u8..=u8::MAX {
            assert_eq!(
                u8::from(AddressType::from(code)),
                code,
                "address {code:#04x}"
            );
        }
    }
}
