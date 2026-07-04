//! Adversarial decoding — hostile input must produce a clean error, never a panic.

mod adversarial {
    use ip::Ipv4Header;

    #[test]
    fn ihl_below_five_does_not_underflow_panic() {
        // ver/IHL = 0x40 (version 4, IHL 0): (0 - 5) would underflow the option-count math.
        let wire = [
            0x40, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x40, 0x11, 0x00, 0x00, 0x0a, 0x00,
            0x00, 0x01, 0x0a, 0x00, 0x00, 0x02,
        ];
        let h = Ipv4Header::decode_exact(&wire).unwrap();
        assert!(h.options.is_empty());
        assert_eq!(h.header_len(), 0);
    }

    #[test]
    fn a_truncated_header_is_a_clean_error() {
        assert!(Ipv4Header::decode_exact(&[0x45, 0x00, 0x00, 0x14]).is_err());
    }

    #[test]
    fn options_past_the_buffer_is_a_clean_error() {
        // IHL 15 claims 40 option bytes, but none follow the 20 fixed bytes.
        let wire = [
            0x4f, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x40, 0x11, 0x00, 0x00, 0x0a, 0x00,
            0x00, 0x01, 0x0a, 0x00, 0x00, 0x02,
        ];
        assert!(Ipv4Header::decode_exact(&wire).is_err());
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        for bytes in [&[][..], &[0xFF], &[0x45; 20], &[0x00; 40], &[0x4f; 24]] {
            let _ = Ipv4Header::decode_exact(bytes); // must not panic
        }
    }
}
