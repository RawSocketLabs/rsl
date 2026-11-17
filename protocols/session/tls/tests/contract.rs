//! Golden wire vectors for the record-layer header, laid out by hand from RFC 5246 §6.2.1:
//! one content-type byte, two version bytes, then a big-endian `uint16` length.

mod contract {
    use tls::{ContentType, ProtocolVersion, RecordHeader};

    #[test]
    fn header_is_type_then_version_then_big_endian_length() {
        let cases: [(RecordHeader, [u8; 5]); 5] = [
            (
                RecordHeader::new(ContentType::ChangeCipherSpec, ProtocolVersion::TLS_1_2, 1),
                [20, 3, 3, 0x00, 0x01],
            ),
            (
                RecordHeader::new(ContentType::Alert, ProtocolVersion::TLS_1_1, 2),
                [21, 3, 2, 0x00, 0x02],
            ),
            (
                RecordHeader::new(ContentType::Handshake, ProtocolVersion::TLS_1_0, 0x01c4),
                [22, 3, 1, 0x01, 0xc4],
            ),
            (
                RecordHeader::new(
                    ContentType::ApplicationData,
                    ProtocolVersion::TLS_1_2,
                    RecordHeader::MAX_PLAINTEXT_LEN,
                ),
                [23, 3, 3, 0x40, 0x00],
            ),
            (
                RecordHeader::new(
                    ContentType::ApplicationData,
                    ProtocolVersion::TLS_1_2,
                    RecordHeader::MAX_CIPHERTEXT_LEN,
                ),
                [23, 3, 3, 0x48, 0x00],
            ),
        ];
        for (header, wire) in cases {
            assert_eq!(header.to_bytes().unwrap(), wire, "{header:?}");
            assert_eq!(RecordHeader::decode_exact(&wire).unwrap(), header);
        }
    }

    #[test]
    fn encoded_header_is_len_bytes() {
        let header = RecordHeader::new(ContentType::Handshake, ProtocolVersion::TLS_1_2, 0);
        assert_eq!(header.to_bytes().unwrap().len(), RecordHeader::LEN);
    }
}
