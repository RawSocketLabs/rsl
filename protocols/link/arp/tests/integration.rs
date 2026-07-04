//! Golden wire vectors — a real ARP packet must decode to the right fields and round-trip
//! byte-identically.

mod integration {
    use arp::{ArpPacket, Operation};
    use ethertype::EtherType;
    use std::net::Ipv4Addr;

    #[test]
    fn decodes_a_request_and_round_trips() {
        // htype 1, ptype IPv4, hlen 6, plen 4, oper 1 (request),
        // sha 02:00:00:00:00:01, spa 10.0.0.1, tha 0, tpa 10.0.0.2.
        let wire = [
            0x00, 0x01, // htype Ethernet
            0x08, 0x00, // ptype IPv4
            0x06, 0x04, // hlen, plen
            0x00, 0x01, // oper request
            0x02, 0x00, 0x00, 0x00, 0x00, 0x01, // sha
            0x0a, 0x00, 0x00, 0x01, // spa 10.0.0.1
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // tha (unknown)
            0x0a, 0x00, 0x00, 0x02, // tpa 10.0.0.2
        ];
        let p = ArpPacket::decode_exact(&wire).unwrap();
        assert_eq!(p.htype, 1);
        assert_eq!(p.ptype, EtherType::IPv4);
        assert_eq!((p.hlen, p.plen), (6, 4));
        assert_eq!(p.oper, Operation::Request);
        assert_eq!(p.sha, [0x02, 0, 0, 0, 0, 1]);
        assert_eq!(p.spa, Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(p.tpa, Ipv4Addr::new(10, 0, 0, 2));
        assert_eq!(p.to_bytes().unwrap(), wire);
    }

    #[test]
    fn an_unknown_operation_is_preserved() {
        // oper 42 is neither request nor reply — dual-use: decodes as Other, round-trips.
        let wire = [
            0x00, 0x01, 0x08, 0x00, 0x06, 0x04, 0x00, 0x2a, // oper = 42
            0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x0a, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x0a, 0x00, 0x00, 0x02,
        ];
        let p = ArpPacket::decode_exact(&wire).unwrap();
        assert_eq!(p.oper, Operation::Other(42));
        assert_eq!(p.to_bytes().unwrap(), wire);
    }
}
