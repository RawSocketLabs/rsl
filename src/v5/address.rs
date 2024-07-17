use std::fmt::{self, Display, Formatter};
use std::net::{Ipv4Addr, Ipv6Addr};

use binrw::binrw;

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
    use super::*;

    #[test]
    fn test_domain() {
        let domain = Domain::from("www.google.com");
        println!("{:?}", domain);
    }
}
