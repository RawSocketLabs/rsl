//! The rawsock injection layer: `Ethernet<P>` framing, the payload-driven EtherType, the full
//! `Ethernet(Ip(Icmp(..)))` stack, and handing the frame to an L2 Loopback sink.
#![cfg(feature = "inject")]

mod inject {
    use ethernet::{BROADCAST, Ethernet, EthernetHeader};
    use ethertype::EtherType;
    use icmp::{Icmp, IcmpHeader};
    use ip::{Ip, Ipv4Header};
    use rawsock::{Layer, Loopback, Protocol, ProtocolExt, RawIo, internet_checksum};
    use std::net::Ipv4Addr;

    fn header() -> EthernetHeader {
        EthernetHeader {
            dst: BROADCAST,
            src: [0x02, 0, 0, 0, 0, 1],
            ethertype: EtherType::Custom(0), // filled on encode
        }
    }

    fn stack() -> Ethernet<Ip<Icmp<Vec<u8>>>> {
        Ethernet::new(
            header(),
            Ip::new(
                Ipv4Header::datagram(Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2), 1, 0),
                Icmp::new(IcmpHeader::echo_request(0x1234, 1), b"pingpong".to_vec()),
            ),
        )
    }

    #[test]
    fn is_the_outermost_link_layer() {
        let s = stack();
        assert_eq!(s.protocol_id(), None); // nothing demuxes L2 upward
        assert!(matches!(s.layer(), Layer::Link));
    }

    #[test]
    fn compliant_encode_sets_the_ethertype_from_the_payload() {
        let bytes = stack().encode();
        assert_eq!(&bytes[12..14], &[0x08, 0x00]); // IPv4, from the IP payload's demux id
    }

    #[test]
    fn raw_encode_preserves_a_forged_ethertype() {
        let raw = Ethernet::new(header(), vec![0xAAu8, 0xBB]).encode_raw();
        // Vec<u8> payload gives no demux hint, and encode_raw never rewrites — Custom(0) stays.
        assert_eq!(&raw[12..14], &[0x00, 0x00]);
        assert_eq!(&raw[14..], &[0xAA, 0xBB]);
    }

    #[test]
    fn full_stack_frames_a_pingable_packet_with_both_checksums() {
        let bytes = stack().encode();
        assert_eq!(&bytes[12..14], &[0x08, 0x00]); // EtherType IPv4
        assert_eq!(bytes[14 + 9], 1); // IP protocol = ICMP
        assert_eq!(internet_checksum(&bytes[14..34]), 0); // IPv4 header
        assert_eq!(internet_checksum(&bytes[34..]), 0); // ICMP message
    }

    #[test]
    fn the_frame_goes_out_an_l2_rawsock_sink() {
        let bytes = stack().encode();
        let mut sink = Loopback::new(Layer::Link);
        assert_eq!(sink.send_raw(&bytes).unwrap(), bytes.len());
        assert_eq!(sink.sent(), &[bytes]);
    }
}
