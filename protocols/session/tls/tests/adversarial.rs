//! Adversarial decoding: record bytes come from the network, so hostile or truncated input
//! must be a clean error or a preserved value, never a panic.

mod adversarial {
    use tls::{ContentType, ProtocolVersion, RecordHeader};

    #[test]
    fn truncated_header_is_reported_as_incomplete() {
        let err = RecordHeader::decode_exact(&[22, 3, 3, 0]).unwrap_err();
        assert!(
            matches!(err.kind, bnb::ErrorKind::UnexpectedEof { .. }),
            "{err}"
        );
    }

    #[test]
    fn unknown_type_version_and_oversized_length_round_trip() {
        let wire = [0xfe, 0x7f, 0x09, 0xff, 0xff];
        let header = RecordHeader::decode_exact(&wire).unwrap();
        assert_eq!(header.content_type, ContentType::Other(0xfe));
        assert_eq!(header.version, ProtocolVersion::new(0x7f, 0x09));
        assert_eq!(header.length, 0xffff);
        assert_eq!(header.to_bytes().unwrap(), wire);
    }
}
