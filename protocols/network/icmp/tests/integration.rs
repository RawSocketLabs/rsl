//! Golden wire vectors — a real ICMP header must decode to the right fields and round-trip
//! byte-identically.

mod integration {
    use icmp::IcmpHeader;

    #[test]
    fn decodes_an_echo_request_and_round_trips() {
        // type 8 (echo request), code 0, checksum 0xf7fe, id 0x1234, seq 1.
        let wire = [0x08, 0x00, 0xf7, 0xfe, 0x12, 0x34, 0x00, 0x01];
        let h = IcmpHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.icmp_type, IcmpHeader::ECHO_REQUEST);
        assert_eq!(h.code, 0);
        assert_eq!(h.checksum, 0xf7fe);
        assert_eq!(h.identifier(), 0x1234);
        assert_eq!(h.sequence(), 1);
        assert_eq!(h.to_bytes().unwrap(), wire);
    }

    #[test]
    fn decodes_a_time_exceeded_header() {
        // type 11 (time exceeded), code 0, checksum 0, unused rest-of-header.
        let wire = [0x0b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let h = IcmpHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.icmp_type, IcmpHeader::TIME_EXCEEDED);
        assert_eq!(h.rest_of_header, 0);
        assert_eq!(h.to_bytes().unwrap(), wire);
    }

    #[test]
    fn a_forged_checksum_survives_the_round_trip() {
        // Echo reply with a deliberately wrong checksum — dual-use: preserved verbatim.
        let wire = [0x00, 0x00, 0xde, 0xad, 0x12, 0x34, 0x00, 0x02];
        let h = IcmpHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.checksum, 0xdead);
        assert_eq!(h.to_bytes().unwrap(), wire);
    }
}
