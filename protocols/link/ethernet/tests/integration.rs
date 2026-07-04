//! Golden wire vectors — a real Ethernet header must decode to the right fields and round-trip
//! byte-identically.

mod integration {
    use ethernet::EthernetHeader;
    use ethertype::EtherType;

    #[test]
    fn decodes_a_frame_header_and_round_trips() {
        // dst 01:02:03:04:05:06, src aa:bb:cc:dd:ee:ff, ethertype 0x0800 (IPv4).
        let wire = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, // dst
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, // src
            0x08, 0x00, // ethertype IPv4
        ];
        let h = EthernetHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.dst, [0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        assert_eq!(h.src, [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        assert_eq!(h.ethertype, EtherType::IPv4);
        assert_eq!(h.to_bytes().unwrap(), wire);
    }

    #[test]
    fn an_unknown_ethertype_is_preserved_as_custom() {
        // Dual-use: an unregistered ethertype decodes as Custom, never rejected.
        let wire = [0; 6]
            .into_iter()
            .chain([0; 6])
            .chain([0x12, 0x34])
            .collect::<Vec<u8>>();
        let h = EthernetHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.ethertype, EtherType::Custom(0x1234));
        assert_eq!(h.to_bytes().unwrap(), wire);
    }
}
