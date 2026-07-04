//! Adversarial decoding — hostile input must produce a clean error, never a panic.

mod adversarial {
    use tcp::TcpHeader;

    #[test]
    fn data_offset_below_five_does_not_underflow_panic() {
        // data_offset=0 (control 0x0000): (0 - 5) would underflow the option-count math.
        // saturating_sub keeps it at zero options; decode must not panic.
        let wire = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, // data_offset=0
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let h = TcpHeader::decode_exact(&wire).unwrap();
        assert!(h.options.is_empty());
        assert_eq!(h.header_len(), 0);
    }

    #[test]
    fn a_truncated_header_is_a_clean_error() {
        // Fewer than the 20 fixed bytes.
        assert!(TcpHeader::decode_exact(&[0x00; 10]).is_err());
    }

    #[test]
    fn options_past_the_buffer_is_a_clean_error() {
        // data_offset=15 claims 40 option bytes, but none follow.
        let wire = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
            0xf0, 0x00, // data_offset=15 → 40 option bytes expected
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert!(TcpHeader::decode_exact(&wire).is_err());
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        for bytes in [&[][..], &[0xFF], &[0xFF; 20], &[0x00; 60], &[0xF0; 24]] {
            let _ = TcpHeader::decode_exact(bytes); // must not panic
        }
    }
}
