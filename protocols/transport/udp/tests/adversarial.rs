//! Adversarial decoding — hostile input must produce a clean error, never a panic.

mod adversarial {
    use udp::UdpHeader;

    #[test]
    fn a_truncated_header_is_a_clean_error() {
        // Fewer than the 8 fixed bytes.
        assert!(UdpHeader::decode_exact(&[0x00; 7]).is_err());
    }

    #[test]
    fn a_length_below_the_header_does_not_underflow() {
        // length = 3 (< the 8-byte header): payload_len saturates to 0, no panic.
        let wire = [0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x00];
        let h = UdpHeader::decode_exact(&wire).unwrap();
        assert_eq!(h.payload_len(), 0);
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        for bytes in [&[][..], &[0xFF], &[0xFF; 8], &[0x00; 20]] {
            let _ = UdpHeader::decode_exact(bytes); // must not panic
        }
    }
}
