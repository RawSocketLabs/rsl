//! Adversarial decoding — hostile input must produce a clean error, never a panic.

mod adversarial {
    use icmp::IcmpHeader;

    #[test]
    fn a_truncated_header_is_a_clean_error() {
        // Fewer than the 8 fixed bytes.
        assert!(IcmpHeader::decode_exact(&[0x08, 0x00, 0x00]).is_err());
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        for bytes in [&[][..], &[0xFF], &[0x08; 8], &[0x00; 20]] {
            let _ = IcmpHeader::decode_exact(bytes); // must not panic
        }
    }
}
