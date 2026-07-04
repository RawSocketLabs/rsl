//! The rawsock injection layer: `ArpPacket` as a leaf `Protocol`, framed in Ethernet, out an
//! L2 Loopback sink.
#![cfg(feature = "inject")]

mod inject {
    use arp::ArpPacket;
    use ethernet::{BROADCAST, Ethernet, EthernetHeader};
    use ethertype::EtherType;
    use rawsock::{Layer, Loopback, Protocol, ProtocolExt, RawIo};
    use std::net::Ipv4Addr;

    fn request() -> ArpPacket {
        ArpPacket::request(
            [0x02, 0, 0, 0, 0, 1],
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(10, 0, 0, 2),
        )
    }

    #[test]
    fn presents_the_arp_ethertype_and_network_layer() {
        let p = request();
        assert_eq!(p.protocol_id(), Some(0x0806));
        assert!(matches!(p.layer(), Layer::Network));
    }

    #[test]
    fn encode_is_verbatim_and_has_no_derived_fields() {
        let p = request();
        // No checksum/length to compute — encode, encode_raw, and to_bytes all agree.
        assert_eq!(p.encode(), p.to_bytes().unwrap());
        assert_eq!(p.encode_raw(), p.encode());
        assert_eq!(p.encode().len(), 28);
    }

    #[test]
    fn framed_in_ethernet_sets_the_ethertype_and_sends() {
        let frame = Ethernet::new(
            EthernetHeader {
                dst: BROADCAST,
                src: [0x02, 0, 0, 0, 0, 1],
                ethertype: EtherType::Custom(0), // filled from the ARP payload
            },
            request(),
        );
        let bytes = frame.encode();
        assert_eq!(&bytes[12..14], &[0x08, 0x06]); // EtherType ARP, from the payload
        assert_eq!(bytes.len(), 14 + 28);

        let mut sink = Loopback::new(Layer::Link);
        assert_eq!(sink.send_raw(&bytes).unwrap(), bytes.len());
        assert_eq!(sink.sent(), &[bytes]);
    }
}
