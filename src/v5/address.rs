use std::fmt::{self, Display, Formatter};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use binrw::{binrw, io::Cursor, BinRead};

use crate::error::{Result, SocksError};

/// A SOCKS5 address-type code (the ATYP field, RFC 1928 §4 Requests).
///
/// IPv4, domain name, and IPv6 are the assigned codes; any other is `Custom`.
//~ models rfc1928#4 registry="ATYP"
#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AddressType {
    /// X'01' IP V4 address.
    #[brw(magic = 0x01u8)]
    V4 = 0x01,

    /// X'03' DOMAINNAME.
    #[brw(magic = 0x03u8)]
    Domain = 0x03,

    /// X'04' IP V6 address.
    #[brw(magic = 0x04u8)]
    V6 = 0x04,

    /// Any unassigned address-type code.
    Custom(u8),
}

#[binrw]
#[brw(big)]
#[br(import {address_type: AddressType})]
#[derive(Clone, Debug)]
pub enum Address {
    #[br(pre_assert(address_type == AddressType::V4))]
    V4(
        #[bw(map = |v: &Ipv4Addr| v.octets())]
        #[br(map = |v: [u8; 4]| Ipv4Addr::from(v))]
        Ipv4Addr,
    ),

    #[br(pre_assert(address_type == AddressType::Domain))]
    Domain(Domain),

    #[br(pre_assert(address_type == AddressType::V6))]
    V6(
        #[bw(map = |v: &Ipv6Addr| v.octets())]
        #[br(map = |v: [u8; 16]| Ipv6Addr::from(v))]
        Ipv6Addr,
    ),
}

impl Address {
    /// Returns the [`AddressType`] discriminant for this address.
    pub fn address_type(&self) -> AddressType {
        match self {
            Address::V4(_) => AddressType::V4,
            Address::Domain(_) => AddressType::Domain,
            Address::V6(_) => AddressType::V6,
        }
    }

    /// Formats the address with a port in a form accepted by
    /// [`std::net::ToSocketAddrs`].
    pub fn to_socket_string(&self, port: u16) -> String {
        match self {
            Address::V4(addr) => format!("{}:{}", addr, port),
            Address::Domain(domain) => format!("{}:{}", domain, port),
            Address::V6(addr) => format!("[{}]:{}", addr, port),
        }
    }
}

/// Reads the `address + port` tail of a message whose last consumed byte is
/// the address type, appending the bytes to `buf` so the complete message can
/// be parsed from a seekable cursor.
///
/// Messages are framed exactly so no bytes beyond the message are consumed
/// from the stream.
pub(crate) fn read_addressed_tail(reader: &mut impl Read, mut buf: Vec<u8>) -> Result<Vec<u8>> {
    let address_type = AddressType::read_be(&mut Cursor::new(&buf[buf.len() - 1..]))?;

    let address_length = match address_type {
        AddressType::V4 => 4,
        AddressType::V6 => 16,
        AddressType::Domain => {
            let mut length = [0u8; 1];
            reader.read_exact(&mut length)?;
            buf.push(length[0]);
            length[0] as usize
        }
        AddressType::Custom(other) => {
            return Err(SocksError::NotSupported(format!(
                "address type {:#04x}",
                other
            )))
        }
    };

    let start = buf.len();
    buf.resize(start + address_length + 2, 0);
    reader.read_exact(&mut buf[start..])?;

    Ok(buf)
}

impl From<IpAddr> for Address {
    fn from(addr: IpAddr) -> Self {
        match addr {
            IpAddr::V4(addr) => Address::V4(addr),
            IpAddr::V6(addr) => Address::V6(addr),
        }
    }
}

impl From<SocketAddr> for Address {
    fn from(addr: SocketAddr) -> Self {
        Address::from(addr.ip())
    }
}

#[binrw]
#[derive(Clone)]
pub struct Domain {
    pub length: u8,

    #[br(count = length)]
    pub domain: Vec<u8>,
}

impl From<String> for Domain {
    fn from(domain: String) -> Self {
        let length = domain.len() as u8;
        let domain = domain.into_bytes();

        Self { length, domain }
    }
}

impl From<&str> for Domain {
    fn from(domain: &str) -> Self {
        let length = domain.len() as u8;
        let domain = domain.as_bytes().to_vec();

        Self { length, domain }
    }
}

impl Display for Domain {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let domain = String::from_utf8_lossy(&self.domain);
        write!(f, "{}", domain)
    }
}

impl fmt::Debug for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let domain = String::from_utf8_lossy(&self.domain);
        write!(f, "{}", domain)
    }
}

#[cfg(test)]
mod unit {
    use binrw::BinWrite;

    use super::*;

    #[test]
    fn test_domain() {
        let domain = Domain::from("www.google.com");
        println!("{:?}", domain);
    }

    fn parse(code: u8) -> AddressType {
        AddressType::read_be(&mut Cursor::new(vec![code])).expect("parses")
    }

    /// The ATYP enum maps the RFC 1928 §4 address-type codes and round-trips
    /// all 256.
    #[test]
    fn enum_matches_address_type_codes() {
        assert_eq!(parse(0x01), AddressType::V4);
        assert_eq!(parse(0x03), AddressType::Domain);
        assert_eq!(parse(0x04), AddressType::V6);
        for code in [0x00u8, 0x02, 0x05, 0xFF] {
            assert_eq!(parse(code), AddressType::Custom(code), "code {code:#04x}");
        }
        for code in 0u8..=0xFF {
            let mut buf = Cursor::new(Vec::new());
            parse(code).write_be(&mut buf).expect("writes");
            assert_eq!(buf.into_inner(), vec![code], "code {code:#04x} did not round-trip");
        }
    }
}
