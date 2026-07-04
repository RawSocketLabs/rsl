//! The rawsock injection layer: `Udp<P>` composition, the pseudo-header checksum, and handing
//! the composed bytes to a rawsock `RawIo` sink (Loopback — no privilege).
#![cfg(feature = "inject")]

mod inject {
    use rawsock::{
        Context, Layer, Loopback, Protocol, ProtocolExt, Pseudo, RawIo, internet_checksum,
    };
    use std::net::Ipv4Addr;
    use udp::Udp;

    fn pseudo() -> Pseudo {
        Pseudo {
            src: Ipv4Addr::new(192, 168, 1, 1),
            dst: Ipv4Addr::new(192, 168, 1, 2),
            protocol: 17,
        }
    }

    #[test]
    fn presents_udp_demux_id_and_transport_layer() {
        let u = Udp::new(1, 2, Vec::new());
        assert_eq!(u.protocol_id(), Some(17));
        assert!(matches!(u.layer(), Layer::Transport));
    }

    #[test]
    fn compliant_encode_computes_length_and_checksum() {
        let u = Udp::new(0x1234, 0x5678, vec![0xDE, 0xAD]);
        let enc = u.encode_with(&Context {
            pseudo: Some(pseudo()),
        });
        assert_eq!(u16::from_be_bytes([enc[4], enc[5]]), 10); // 8 header + 2 payload
        assert_ne!(&enc[6..8], &[0, 0]); // a real checksum was written
    }

    #[test]
    fn the_computed_checksum_verifies_to_zero() {
        // RFC 1071: summing the pseudo-header + UDP datagram (checksum in place) is all-ones,
        // so `internet_checksum` over it yields 0. The correctness proof.
        let p = pseudo();
        let enc =
            Udp::new(0x1234, 0x5678, vec![0xDE, 0xAD]).encode_with(&Context { pseudo: Some(p) });
        let mut buf = Vec::new();
        buf.extend_from_slice(&p.src.octets());
        buf.extend_from_slice(&p.dst.octets());
        buf.extend_from_slice(&[0, p.protocol]);
        buf.extend_from_slice(&u16::try_from(enc.len()).unwrap().to_be_bytes());
        buf.extend_from_slice(&enc);
        assert_eq!(internet_checksum(&buf), 0);
    }

    #[test]
    fn without_a_pseudo_header_the_checksum_stays_zero() {
        // A top-level encode has no L3 context — 0 is a legal "no checksum" for IPv4 UDP.
        let enc = Udp::new(0x1234, 0x5678, vec![0xDE, 0xAD]).encode();
        assert_eq!(&enc[6..8], &[0, 0]);
        assert_eq!(u16::from_be_bytes([enc[4], enc[5]]), 10);
    }

    #[test]
    fn raw_encode_preserves_forged_length_and_checksum() {
        let mut u = Udp::new(1, 2, vec![0xAA]);
        u.header.length = 9999; // a lie
        u.header.checksum = 0xBEEF; // a lie
        assert_eq!(
            u.encode_raw(),
            vec![0, 1, 0, 2, 0x27, 0x0F, 0xBE, 0xEF, 0xAA]
        );
    }

    #[test]
    fn composed_packet_goes_out_a_rawsock_sink() {
        // The milestone: a real, checksummed UDP packet handed to a rawsock RawIo backend.
        let bytes = Udp::new(40000, 53, vec![1, 2, 3, 4]).encode_with(&Context {
            pseudo: Some(pseudo()),
        });
        let mut sink = Loopback::new(Layer::Network);
        let n = sink.send_raw(&bytes).unwrap();
        assert_eq!(n, bytes.len());
        assert_eq!(sink.sent(), &[bytes]);
    }
}
