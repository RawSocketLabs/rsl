//! The rawsock injection layer: `Tcp<P>` composition, the pseudo-header checksum, and handing
//! the composed bytes to a rawsock `RawIo` sink (Loopback — no privilege).
#![cfg(feature = "inject")]

mod inject {
    use rawsock::{
        Context, Layer, Loopback, Protocol, ProtocolExt, Pseudo, RawIo, internet_checksum,
    };
    use std::net::Ipv4Addr;
    use tcp::{Control, Tcp, TcpHeader};

    fn pseudo() -> Pseudo {
        Pseudo {
            src: Ipv4Addr::new(192, 168, 1, 1),
            dst: Ipv4Addr::new(192, 168, 1, 2),
            protocol: 6,
        }
    }

    fn syn() -> Tcp<Vec<u8>> {
        let h = TcpHeader::segment(
            0x1234,
            0x5678,
            1,
            0,
            Control::new().with_syn(true),
            8192,
            vec![],
        );
        Tcp::new(h, Vec::new())
    }

    #[test]
    fn presents_tcp_demux_id_and_transport_layer() {
        let s = syn();
        assert_eq!(s.protocol_id(), Some(6));
        assert!(matches!(s.layer(), Layer::Transport));
    }

    #[test]
    fn the_computed_checksum_verifies_to_zero() {
        // RFC 1071: pseudo-header + segment (checksum in place) sums to all-ones → 0.
        let p = pseudo();
        let seg = Tcp::new(
            TcpHeader::segment(
                0x1234,
                0x5678,
                1,
                0,
                Control::new().with_ack(true),
                8192,
                vec![],
            ),
            vec![0xDE, 0xAD, 0xBE, 0xEF],
        );
        let enc = seg.encode_with(&Context { pseudo: Some(p) });
        assert_ne!(&enc[16..18], &[0, 0]); // a real checksum was written
        let mut buf = Vec::new();
        buf.extend_from_slice(&p.src.octets());
        buf.extend_from_slice(&p.dst.octets());
        buf.extend_from_slice(&[0, p.protocol]);
        buf.extend_from_slice(&u16::try_from(enc.len()).unwrap().to_be_bytes());
        buf.extend_from_slice(&enc);
        assert_eq!(internet_checksum(&buf), 0);
    }

    #[test]
    fn raw_encode_preserves_the_forged_checksum() {
        let mut h = TcpHeader::segment(1, 2, 0, 0, Control::new(), 0, vec![]);
        h.checksum = 0xBEEF; // a lie
        let raw = Tcp::new(h, Vec::new()).encode_raw();
        assert_eq!(&raw[16..18], &[0xBE, 0xEF]); // preserved verbatim
    }

    #[test]
    fn segment_data_rides_after_the_header() {
        let seg = Tcp::new(
            TcpHeader::segment(1, 2, 0, 0, Control::new().with_psh(true), 0, vec![]),
            vec![b'h', b'i'],
        );
        let enc = seg.encode();
        assert_eq!(&enc[enc.len() - 2..], b"hi");
    }

    #[test]
    fn composed_segment_goes_out_a_rawsock_sink() {
        let bytes = syn().encode_with(&Context {
            pseudo: Some(pseudo()),
        });
        let mut sink = Loopback::new(Layer::Network);
        let n = sink.send_raw(&bytes).unwrap();
        assert_eq!(n, bytes.len());
        assert_eq!(sink.sent(), &[bytes]);
    }
}
