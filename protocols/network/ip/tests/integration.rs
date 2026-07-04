//! Golden wire vectors — a real IPv4 header must decode to the right fields and round-trip
//! byte-identically.

mod integration {
    use ip::Ipv4Header;
    use std::net::Ipv4Addr;

    #[test]
    fn decodes_a_header_and_round_trips() {
        // ver/IHL 0x45, DSCP 0, total_length 60, id 0x1c46, DF set, TTL 64, proto 6 (TCP),
        // checksum 0xb1e6, src 192.168.0.1, dst 192.168.0.199.
        let wire = [
            0x45, 0x00, 0x00, 0x3c, // ver/ihl, dscp, total_length
            0x1c, 0x46, 0x40, 0x00, // id, flags+frag (DF)
            0x40, 0x06, 0xb1, 0xe6, // ttl, proto, checksum
            0xc0, 0xa8, 0x00, 0x01, // src 192.168.0.1
            0xc0, 0xa8, 0x00, 0xc7, // dst 192.168.0.199
        ];
        let h = Ipv4Header::decode_exact(&wire).unwrap();
        assert_eq!(u8::from(h.version_ihl.version()), 4);
        assert_eq!(h.header_len(), 20);
        assert_eq!(h.total_length, 60);
        assert_eq!(h.identification, 0x1c46);
        assert!(h.flags_fragment.dont_fragment());
        assert_eq!(h.ttl, 64);
        assert_eq!(h.protocol, 6);
        assert_eq!(h.header_checksum, 0xb1e6);
        assert_eq!(h.src, Ipv4Addr::new(192, 168, 0, 1));
        assert_eq!(h.dst, Ipv4Addr::new(192, 168, 0, 199));
        assert!(h.options.is_empty());
        assert_eq!(h.to_bytes().unwrap(), wire);
    }

    #[test]
    fn a_forged_total_length_survives_the_round_trip() {
        // total_length claims 60000 with no such payload here — dual-use: preserved verbatim.
        let wire = [
            0x45, 0x00, 0xea, 0x60, 0x00, 0x00, 0x00, 0x00, 0x40, 0x11, 0x00, 0x00, 0x0a, 0x00,
            0x00, 0x01, 0x0a, 0x00, 0x00, 0x02,
        ];
        let h = Ipv4Header::decode_exact(&wire).unwrap();
        assert_eq!(h.total_length, 60000);
        assert_eq!(h.to_bytes().unwrap(), wire);
    }
}
