use std::net::{Ipv4Addr, Ipv6Addr};

use binrw::binrw;
use derive_builder::Builder;

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Identifier {
    #[builder(default = "5")]
    pub version: u8,

    #[builder(setter(into, strip_option))]
    #[bw(map = |n| n.unwrap_or(methods.len() as u8))]
    pub number_of_methods: Option<u8>,

    #[br(count = number_of_methods.unwrap_or(0))]
    pub methods: Vec<Method>,
}

pub struct Offer {
    pub version: u8,
    pub method: Method,
}

#[binrw]
#[brw(repr = u8)]
#[derive(Clone, Debug)]
pub enum Method {
    NoAuth = 0x00,
    GssApi = 0x01,
    Plain = 0x02,
    IanaReserved = 0x03,
    PrivateMethods = 0x80,
    NoAcceptableMethods = 0xFF,
}

pub struct Request {
    pub version: u8,
    pub command: Method,
    pub reserved: u8,
    pub address_type: AddressType,
    pub dest_addr: Address,
    pub dest_port: u16,
}

pub enum Command {
    Connect,
    Bind,
    UdpAssociate,
}

pub enum AddressType {
    V4,
    Domain,
    V6,
}

pub enum Address {
    V4(Ipv4Addr),
    Domain(String),
    V6(Ipv6Addr),
}

pub struct Reply {
    pub version: u8,
    pub reply: Response,
    pub reserved: u8,
    pub address_type: AddressType,
    pub bind_addr: Address,
}

pub enum Response {
    Succeeded,
    GeneralFailure,
    ConnectionNotAllowed,
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    TtlExpired,
    CommandNotSupported,
    AddressTypeNotSupported,
}

#[cfg(test)]
mod unit {
    use binrw::{BinRead, BinWrite};

    use super::*;

    #[test]
    fn test_identifier() {
        let id = IdentifierBuilder::default()
            .number_of_methods(1)
            .methods(vec![Method::NoAuth])
            .build()
            .unwrap();
        println!("{:?}", id);
    }
}
