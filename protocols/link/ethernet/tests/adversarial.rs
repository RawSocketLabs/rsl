//! Adversarial decoding — hostile input must produce a clean error, never a panic.

mod adversarial {
    use ethernet::EthernetHeader;

    #[test]
    fn a_truncated_header_is_a_clean_error() {
        // Fewer than the 14 fixed bytes.
        assert!(EthernetHeader::decode_exact(&[0x00; 13]).is_err());
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        for bytes in [&[][..], &[0xFF], &[0xFF; 14], &[0x00; 32]] {
            let _ = EthernetHeader::decode_exact(bytes); // must not panic
        }
    }
}
