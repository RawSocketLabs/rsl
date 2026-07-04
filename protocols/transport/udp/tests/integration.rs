//! Golden wire vectors — a real UDP header must decode to the right fields and round-trip
//! byte-identically.

mod integration {
    use udp::UdpHeader;

    #[test]
    fn decodes_a_datagram_header_and_round_trips() {
        // src 40000, dst 53 (DNS), length 37 (8 header + 29 payload), checksum 0xABCD.
        let wire = [
            0x9c, 0x40, // src_port 40000
            0x00, 0x35, // dst_port 53
            0x00, 0x25, // length 37
            0xab, 0xcd, // checksum
        ];
        let h = UdpHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.src_port, 40000);
        assert_eq!(h.dst_port, 53);
        assert_eq!(h.length, 37);
        assert_eq!(h.checksum, 0xABCD);
        assert_eq!(h.payload_len(), 29);
        assert_eq!(h.to_bytes().unwrap(), wire);
    }

    #[test]
    fn a_forged_length_survives_the_round_trip() {
        // length says 9000 but there's no payload here — dual-use: preserved verbatim.
        let wire = [0x00, 0x01, 0x00, 0x02, 0x23, 0x28, 0x00, 0x00];
        let h = UdpHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.length, 9000);
        assert_eq!(h.to_bytes().unwrap(), wire);
    }
}
