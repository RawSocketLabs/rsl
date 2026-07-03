//! Contract tests — golden wire vectors carried from the reference implementation, the
//! decode-fidelity anchor for the bnb rewrite. Real DNS bytes must decode to the correct
//! structure; uncompressed messages must round-trip byte-identically.

mod integration {
    use dns::{Message, RData, RType};
    use std::net::Ipv4Addr;

    /// An `example.com`/`www.example.com` A response, **uncompressed** on the wire.
    #[test]
    fn uncompressed_a_response_decodes_and_round_trips() {
        let wire = [
            0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x07, b'e',
            b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00,
            0x01, 0x03, b'w', b'w', b'w', 0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03,
            b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04,
            0x01, 0x02, 0x03, 0x04,
        ];
        let msg = Message::decode_exact(&wire).unwrap();

        assert_eq!(msg.header.id, 0x1234);
        assert!(msg.header.is_response());
        assert_eq!(msg.questions.len(), 1);
        assert_eq!(msg.questions[0].name.to_string(), "example.com");
        assert_eq!(msg.questions[0].qtype, dns::QType::A);

        assert_eq!(msg.answers.len(), 1);
        assert_eq!(msg.answers[0].name.to_string(), "www.example.com");
        assert_eq!(msg.answers[0].rtype, RType::A);
        assert_eq!(msg.answers[0].ttl, 0x3c);
        assert_eq!(msg.answers[0].data, RData::A(Ipv4Addr::new(1, 2, 3, 4)));

        // Uncompressed in → uncompressed out, byte-identical.
        assert_eq!(msg.to_bytes().unwrap(), wire);
    }

    /// A **compressed** response (`0xC0` pointers + a CNAME whose RDATA is a pointer).
    /// Decode must resolve every name inline; re-encode is uncompressed (not byte-equal).
    #[test]
    fn compressed_response_resolves_names_inline() {
        let wire = [
            0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x07, b'e',
            b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00,
            0x01, 0x03, b'w', b'w', b'w', 0xc0, 0x0c, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x3c, 0x00, 0x02, 0xc0, 0x1d, 0xc0, 0x1d, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x3c, 0x00, 0x04, 0x01, 0x02, 0x03, 0x04,
        ];
        let msg = Message::decode_exact(&wire).unwrap();

        assert_eq!(msg.questions[0].name.to_string(), "example.com");
        assert_eq!(msg.answers.len(), 2);

        // Answer 1: name `www` + pointer(0x0C=example.com) → www.example.com; CNAME RDATA
        // is a pointer(0x1D) → www.example.com.
        assert_eq!(msg.answers[0].name.to_string(), "www.example.com");
        assert_eq!(msg.answers[0].rtype, RType::CNAME);
        let RData::Cname(target) = &msg.answers[0].data else {
            panic!("expected CNAME, got {:?}", msg.answers[0].data);
        };
        assert_eq!(target.to_string(), "www.example.com");

        // Answer 2: name pointer(0x1D) → www.example.com; A = 1.2.3.4.
        assert_eq!(msg.answers[1].name.to_string(), "www.example.com");
        assert_eq!(msg.answers[1].data, RData::A(Ipv4Addr::new(1, 2, 3, 4)));

        // A decoded compressed message re-encodes uncompressed — and that uncompressed
        // form itself round-trips.
        let reencoded = msg.to_bytes().unwrap();
        assert_eq!(Message::decode_exact(&reencoded).unwrap(), msg);
    }

    /// An unknown record type keeps its raw RDATA (dual-use: never misparsed).
    #[test]
    fn unknown_rtype_preserves_raw_rdata() {
        // Header (qd=0 an=1), then: root name, TYPE=9999, CLASS=IN, TTL=0, RDLENGTH=3, 3 bytes.
        let wire = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, // header
            0x00, // root name
            0x27, 0x0f, // TYPE = 9999
            0x00, 0x01, // CLASS = IN
            0x00, 0x00, 0x00, 0x00, // TTL
            0x00, 0x03, // RDLENGTH = 3
            0xDE, 0xAD, 0xBE, // RDATA
        ];
        let msg = Message::decode_exact(&wire).unwrap();
        assert_eq!(
            msg.answers[0].data,
            RData::Custom {
                rtype: RType::Custom(9999),
                bytes: vec![0xDE, 0xAD, 0xBE],
            }
        );
        assert_eq!(msg.to_bytes().unwrap(), wire); // raw bytes survive a round-trip
    }
}
