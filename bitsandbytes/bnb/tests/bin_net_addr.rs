//! `std::net::Ipv4Addr`/`Ipv6Addr` as `#[bin]` fields — the foreign-type codec impls follow the
//! struct's byte order like any integer, so a `big` message writes them in network order.
#![cfg(feature = "std")]

mod macro_ {
    use bnb::bin;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Endpoints {
        v4: Ipv4Addr,
        v6: Ipv6Addr,
    }

    #[test]
    fn addresses_serialize_in_network_order_and_round_trip() {
        let e = Endpoints {
            v4: Ipv4Addr::new(192, 168, 1, 1),
            v6: Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 1),
        };
        let bytes = e.to_bytes().unwrap();
        assert_eq!(bytes.len(), 4 + 16);
        assert_eq!(&bytes[..4], &[0xc0, 0xa8, 0x01, 0x01]); // v4 in network order
        assert_eq!(&bytes[4..6], &[0x20, 0x01]); // v6 leading hextet
        assert_eq!(&bytes[18..20], &[0x00, 0x01]); // v6 trailing hextet
        assert_eq!(Endpoints::decode_exact(&bytes).unwrap(), e);
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct WithHost {
        addr: Ipv4Addr,
        port: u16,
    }

    #[test]
    fn an_address_composes_with_other_fields() {
        let w = WithHost {
            addr: Ipv4Addr::new(10, 0, 0, 2),
            port: 53,
        };
        let bytes = w.to_bytes().unwrap();
        assert_eq!(bytes, [0x0a, 0x00, 0x00, 0x02, 0x00, 0x35]);
        assert_eq!(WithHost::decode_exact(&bytes).unwrap(), w);
    }
}
