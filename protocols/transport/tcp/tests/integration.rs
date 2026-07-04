//! Golden wire vectors — real TCP headers must decode to the right fields and round-trip
//! byte-identically.

mod integration {
    use tcp::{TcpHeader, TcpOption};

    #[test]
    fn options_parse_into_a_typed_view() {
        // data_offset=7 (0x7002 = SYN, 28-byte header) with 8 option bytes: MSS + WScale + NOP.
        let wire = [
            0x9c, 0x40, 0x00, 0x50, 0x12, 0x34, 0x56, 0x78, 0x00, 0x00, 0x00, 0x00, //
            0x70, 0x02, // data_offset=7, SYN
            0xff, 0xff, 0x00, 0x00, 0x00, 0x00, //
            0x02, 0x04, 0x05, 0xb4, // MSS 1460
            0x03, 0x03, 0x08, // WScale 8
            0x01, // NOP (pads the options to a 4-byte boundary)
        ];
        let h = TcpHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.header_len(), 28);
        assert_eq!(
            h.options_parsed(),
            vec![
                TcpOption::Mss(1460),
                TcpOption::WindowScale(8),
                TcpOption::Nop,
            ]
        );
        // The raw bytes are still the source of truth for a byte-identical round-trip.
        assert_eq!(h.to_bytes().unwrap(), wire);
    }

    #[test]
    fn decodes_a_syn_and_round_trips() {
        // src 40000, dst 80, seq 0x12345678, ack 0, data_offset=5 + SYN (0x5002),
        // window 0xFFFF, checksum 0, urgent 0. No options.
        let wire = [
            0x9c, 0x40, // src_port 40000
            0x00, 0x50, // dst_port 80
            0x12, 0x34, 0x56, 0x78, // seq
            0x00, 0x00, 0x00, 0x00, // ack
            0x50, 0x02, // data_offset=5, SYN
            0xff, 0xff, // window
            0x00, 0x00, // checksum
            0x00, 0x00, // urgent
        ];
        let h = TcpHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.src_port, 40000);
        assert_eq!(h.dst_port, 80);
        assert_eq!(h.seq, 0x1234_5678);
        assert_eq!(h.ack, 0);
        assert!(h.is_syn());
        assert!(!h.is_ack());
        assert_eq!(h.window, 0xFFFF);
        assert_eq!(h.header_len(), 20);
        assert!(h.options.is_empty());
        // Round-trips byte-identically.
        assert_eq!(h.to_bytes().unwrap(), wire);
    }

    #[test]
    fn decodes_a_header_with_an_mss_option() {
        // data_offset=6 (0x6002 = SYN, 24-byte header) + a 4-byte MSS option (kind 2, len 4,
        // value 1460).
        let wire = [
            0x9c, 0x40, 0x00, 0x50, 0x12, 0x34, 0x56, 0x78, 0x00, 0x00, 0x00, 0x00, //
            0x60, 0x02, // data_offset=6, SYN
            0xff, 0xff, 0x00, 0x00, 0x00, 0x00, //
            0x02, 0x04, 0x05, 0xb4, // MSS = 1460, raw
        ];
        let h = TcpHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.header_len(), 24);
        assert_eq!(h.options, vec![0x02, 0x04, 0x05, 0xb4]);
        assert_eq!(h.to_bytes().unwrap(), wire);
    }
}
