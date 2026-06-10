use binrw::binrw;
use derive_builder::Builder;

use crate::v5::address::{Address, AddressType};

/// UDP request header: RSV, FRAG, ATYP, DST.ADDR, DST.PORT (RFC 1928 §7).
///
/// Prefixes every datagram relayed through a UDP ASSOCIATE relay; the
/// payload follows the header on the wire.
//~ models rfc1928#7
#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct UdpHeader {
    #[builder(default = "0")]
    pub reserved: u16,
    #[builder(default = "0")]
    pub frag: u8,
    pub address_type: AddressType,
    #[br(args { address_type })]
    pub dest_addr: Address,
    pub dest_port: u16,
}

#[cfg(test)]
mod unit {
    use std::net::Ipv4Addr;

    use binrw::{io::Cursor, BinRead, BinWrite};

    use super::*;

    #[test]
    fn test_udp_header_roundtrip() {
        let header = UdpHeaderBuilder::default()
            .address_type(AddressType::V4)
            .dest_addr(Address::V4(Ipv4Addr::new(127, 0, 0, 1)))
            .dest_port(8080)
            .build()
            .unwrap();

        let mut cursor = Cursor::new(Vec::new());
        header.write(&mut cursor).unwrap();

        let bytes = cursor.into_inner();
        assert_eq!(
            bytes,
            vec![0x00, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0x1F, 0x90]
        );

        let mut cursor = Cursor::new(bytes);
        let parsed = UdpHeader::read(&mut cursor).expect("parses");
        assert_eq!(parsed.frag, 0);
        assert_eq!(parsed.dest_port, 8080);
    }
}
