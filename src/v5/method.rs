use std::fmt::{self, Display, Formatter};
use std::net::{Ipv4Addr, Ipv6Addr};

use binrw::binrw;
use derive_builder::Builder;

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Identifier {
    #[builder(default = "5")]
    pub version: u8,

    #[builder(default = "None")]
    #[builder(setter(into, strip_option))]
    #[bw(map = |n| n.unwrap_or(methods.len() as u8))]
    pub number_of_methods: Option<u8>,

    #[br(count = number_of_methods.unwrap_or(0))]
    pub methods: Vec<Method>,
}

#[binrw]
#[derive(Builder, Clone, Debug)]
pub struct Offer {
    #[builder(default = "5")]
    pub version: u8,
    pub method: Method,
}

#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Method {
    #[brw(magic = 0x00u8)]
    NoAuth = 0x00,

    #[brw(magic = 0x01u8)]
    GssApi = 0x01,

    #[brw(magic = 0x02u8)]
    Plain = 0x02,

    #[brw(magic = 0x03u8)]
    IanaReserved = 0x03,

    #[brw(magic = 0x80u8)]
    PrivateMethods = 0x80,

    Custom(u8),

    #[brw(magic = 0xFFu8)]
    NoAcceptableMethods = 0xFF,
}

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Request {
    #[builder(default = "5")]
    pub version: u8,
    pub command: Command,
    #[builder(default = "0")]
    pub reserved: u8,
    pub address_type: AddressType,
    #[br(args { address_type })]
    pub dest_addr: Address,
    pub dest_port: u16,
}

#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Command {
    #[brw(magic = 0x01u8)]
    Connect = 0x01,

    #[brw(magic = 0x02u8)]
    Bind = 0x02,

    #[brw(magic = 0x03u8)]
    UdpAssociate = 0x03,

    Custom(u8),
}

#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AddressType {
    #[brw(magic = 0x01u8)]
    V4 = 0x01,

    #[brw(magic = 0x03u8)]
    Domain = 0x03,

    #[brw(magic = 0x04u8)]
    V6 = 0x04,

    Custom(u8),
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

#[binrw]
#[derive(Builder, Clone, Debug)]
pub struct Reply {
    #[builder(default = "5")]
    pub version: u8,
    pub reply: Response,
    #[builder(default = "0")]
    pub reserved: u8,
    pub address_type: AddressType,
    #[br(args {address_type })]
    pub bind_addr: Address,
    pub bind_port: u16,
}

#[repr(u8)]
#[binrw]
#[derive(Clone, Debug)]
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

    #[test]
    fn test_identifier() {
        let id = IdentifierBuilder::default()
            .methods(vec![Method::NoAuth, Method::GssApi])
            .build()
            .unwrap();
        println!("{:?}", id);

        let mut cursor = Cursor::new(Vec::new());

        id.write(&mut cursor).unwrap();

        println!("{:?}", cursor.into_inner());
    }

    #[test]
    fn test_domain() {
        let domain = Domain::from("www.google.com");
        println!("{:?}", domain);
    }
}
