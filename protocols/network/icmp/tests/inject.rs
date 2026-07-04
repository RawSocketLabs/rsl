//! The rawsock injection layer: `Icmp<P>` composition, the self-contained checksum, the full
//! `Ip(Icmp(..))` stack, and handing the bytes to a Loopback sink.
#![cfg(feature = "inject")]

mod inject {
    use icmp::{Icmp, IcmpHeader};
    use ip::{Ip, Ipv4Header};
    use rawsock::{Context, Layer, Loopback, Protocol, ProtocolExt, RawIo, internet_checksum};
    use std::net::Ipv4Addr;

    fn echo() -> Icmp<Vec<u8>> {
        Icmp::new(IcmpHeader::echo_request(0x1234, 1), b"pingpong".to_vec())
    }

    #[test]
    fn presents_icmp_protocol_and_transport_layer() {
        let e = echo();
        assert_eq!(e.protocol_id(), Some(1));
        assert!(matches!(e.layer(), Layer::Transport));
    }

    #[test]
    fn the_checksum_is_self_contained_and_verifies_to_zero() {
        // Computed with NO pseudo-header (a bare Context) — over the ICMP message alone.
        let msg = echo().encode_with(&Context::default());
        assert_ne!(&msg[2..4], &[0, 0]); // a real checksum was written
        assert_eq!(internet_checksum(&msg), 0); // RFC 1071: the whole message sums to 0
    }

    #[test]
    fn raw_encode_preserves_a_forged_checksum() {
        let mut h = IcmpHeader::echo_reply(1, 2);
        h.checksum = 0xBEEF; // a lie
        let raw = Icmp::new(h, Vec::new()).encode_raw();
        assert_eq!(&raw[2..4], &[0xBE, 0xEF]);
    }

    #[test]
    fn full_ip_icmp_stack_both_checksums_verify() {
        let packet = Ip::new(
            Ipv4Header::datagram(Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2), 1, 0),
            echo(),
        );
        let bytes = packet.encode();
        assert_eq!(bytes[9], 1); // IP protocol = ICMP
        assert_eq!(internet_checksum(&bytes[..20]), 0); // IPv4 header checksum
        assert_eq!(internet_checksum(&bytes[20..]), 0); // ICMP message checksum
    }

    #[test]
    fn composed_message_goes_out_a_rawsock_sink() {
        let bytes = echo().encode();
        let mut sink = Loopback::new(Layer::Network);
        assert_eq!(sink.send_raw(&bytes).unwrap(), bytes.len());
        assert_eq!(sink.sent(), &[bytes]);
    }
}
