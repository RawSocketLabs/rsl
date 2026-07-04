//! The rawsock injection layer: `Ip<P>` wrapping a real UDP layer, so the full stack computes
//! both the IPv4 header checksum and the UDP checksum, then goes out a Loopback sink.
#![cfg(feature = "inject")]

mod inject {
    use ip::{Ip, Ipv4Header};
    use rawsock::{Context, Layer, Loopback, Protocol, ProtocolExt, RawIo, internet_checksum};
    use std::net::Ipv4Addr;
    use udp::Udp;

    fn stack() -> Ip<Udp<Vec<u8>>> {
        Ip::new(
            Ipv4Header::datagram(
                Ipv4Addr::new(10, 0, 0, 1),
                Ipv4Addr::new(10, 0, 0, 2),
                17,
                0,
            ),
            Udp::new(40000, 53, vec![0xC0, 0xFF, 0xEE, 0x00]),
        )
    }

    #[test]
    fn presents_ipv4_ethertype_and_network_layer() {
        let s = stack();
        assert_eq!(s.protocol_id(), Some(0x0800));
        assert!(matches!(s.layer(), Layer::Network));
    }

    #[test]
    fn full_stack_fills_length_protocol_and_both_checksums() {
        let bytes = stack().encode();
        assert_eq!(bytes.len(), 32); // 20 IP + 8 UDP + 4 payload
        // IP: version 4 / IHL 5, total_length 32, protocol 17 (from the UDP payload).
        assert_eq!(bytes[0], 0x45);
        assert_eq!(u16::from_be_bytes([bytes[2], bytes[3]]), 32);
        assert_eq!(bytes[9], 17);
        // The IPv4 header checksum verifies to 0 over the 20-byte header (RFC 1071).
        assert_eq!(internet_checksum(&bytes[..20]), 0);
        // The UDP checksum verifies to 0 over the pseudo-header + UDP datagram — proof the IP
        // layer handed the right pseudo-header down.
        let udp = &bytes[20..];
        let mut buf = Vec::new();
        buf.extend_from_slice(&Ipv4Addr::new(10, 0, 0, 1).octets());
        buf.extend_from_slice(&Ipv4Addr::new(10, 0, 0, 2).octets());
        buf.extend_from_slice(&[0, 17]);
        buf.extend_from_slice(&u16::try_from(udp.len()).unwrap().to_be_bytes());
        buf.extend_from_slice(udp);
        assert_eq!(internet_checksum(&buf), 0);
    }

    #[test]
    fn raw_encode_preserves_forged_header_fields() {
        let mut h = Ipv4Header::datagram(Ipv4Addr::LOCALHOST, Ipv4Addr::LOCALHOST, 17, 0);
        h.total_length = 1234; // a lie
        h.header_checksum = 0xBEEF; // a lie
        let raw = Ip::new(h, Udp::new(1, 2, vec![0xAA])).encode_raw_with(&Context::default());
        assert_eq!(u16::from_be_bytes([raw[2], raw[3]]), 1234); // preserved
        assert_eq!(u16::from_be_bytes([raw[10], raw[11]]), 0xBEEF); // preserved
    }

    #[test]
    fn full_datagram_goes_out_a_rawsock_sink() {
        let bytes = stack().encode();
        let mut sink = Loopback::new(Layer::Network);
        assert_eq!(sink.send_raw(&bytes).unwrap(), bytes.len());
        assert_eq!(sink.sent(), &[bytes]);
    }
}
