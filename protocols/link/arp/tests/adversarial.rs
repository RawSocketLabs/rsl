//! Adversarial decoding — hostile input must produce a clean error, never a panic.

mod adversarial {
    use arp::ArpPacket;

    #[test]
    fn a_truncated_packet_is_a_clean_error() {
        // Fewer than the 28 fixed bytes.
        assert!(ArpPacket::decode_exact(&[0x00; 27]).is_err());
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        for bytes in [&[][..], &[0xFF], &[0x00; 28], &[0xFF; 40]] {
            let _ = ArpPacket::decode_exact(bytes); // must not panic
        }
    }
}
