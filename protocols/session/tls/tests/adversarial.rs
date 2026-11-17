//! Adversarial decoding: every byte here comes from the network, so hostile or truncated
//! input must be a clean error or a preserved value, never a panic and never a silent
//! misreading of where a list ends.

mod support;

mod adversarial {
    use bnb::ErrorKind;
    use tls::wire::{
        Certificate, CipherSuite, ClientHello, ClientKeyExchange, ContentType, Extension,
        ExtensionType, Finished, ProtocolVersion, RecordHeader, ServerHello, ServerKeyExchange,
    };

    use super::support::{self, hex};

    /// A hello body up to and including `session_id`, followed by `rest`.
    fn client_hello_with(rest: &[u8]) -> Vec<u8> {
        let mut wire = vec![3, 3];
        wire.extend([0x11; 32]);
        wire.push(0);
        wire.extend(rest);
        wire
    }

    #[test]
    fn truncated_record_header_is_reported_as_incomplete() {
        let err = RecordHeader::decode_exact(&[22, 3, 3, 0]).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }), "{err}");
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

    #[test]
    fn unknown_suite_and_extension_round_trip() {
        let wire = client_hello_with(&[0, 2, 0x13, 0x37, 1, 0, 0, 5, 0xab, 0xcd, 0, 1, 0x99]);
        let hello = ClientHello::decode_exact(&wire).unwrap();
        assert_eq!(hello.cipher_suites, [CipherSuite::Other(0x1337)]);
        assert_eq!(
            hello.extensions,
            Some(vec![Extension::new(
                ExtensionType::Other(0xabcd),
                vec![0x99]
            )])
        );
        assert_eq!(hello.to_bytes().unwrap(), wire);
    }

    /// The error kind a hello body is rejected with.
    fn client_hello_error(wire: &[u8]) -> ErrorKind {
        ClientHello::decode_exact(wire).unwrap_err().kind
    }

    #[test]
    fn suite_list_with_an_odd_byte_length_is_an_error() {
        let wire = client_hello_with(&[0, 3, 0xc0, 0x2f, 0x00, 1, 0]);
        assert_eq!(
            client_hello_error(&wire),
            ErrorKind::UnexpectedEof {
                needed: 16,
                remaining: 8
            },
            "the third byte is half a suite"
        );
    }

    #[test]
    fn suite_list_longer_than_the_message_is_an_error() {
        let wire = client_hello_with(&[0, 8, 0xc0, 0x2f, 1, 0]);
        assert!(matches!(
            client_hello_error(&wire),
            ErrorKind::UnexpectedEof { remaining: 0, .. }
        ));
    }

    #[test]
    fn one_stray_byte_after_compression_methods_is_an_error_not_an_absent_block() {
        let wire = client_hello_with(&[0, 2, 0xc0, 0x2f, 1, 0, 0x00]);
        assert_eq!(
            client_hello_error(&wire),
            ErrorKind::UnexpectedEof {
                needed: 16,
                remaining: 8
            },
            "one byte cannot be an extension-block length"
        );
    }

    #[test]
    fn extension_data_running_past_its_block_is_an_error() {
        // The block is 5 bytes; its extension claims 2 data bytes, and only 1 is inside the
        // block. The second (0x42) is in the message but belongs to nothing.
        let wire = client_hello_with(&[0, 2, 0xc0, 0x2f, 1, 0, 0, 5, 0, 0, 0, 2, 0x41, 0x42]);
        assert_eq!(
            client_hello_error(&wire),
            ErrorKind::UnexpectedEof {
                needed: 8,
                remaining: 0
            },
            "the block, not the message, bounds the extension"
        );
    }

    #[test]
    fn bytes_after_the_extension_block_are_an_error() {
        let wire = client_hello_with(&[0, 2, 0xc0, 0x2f, 1, 0, 0, 0, 0xff]);
        assert_eq!(
            client_hello_error(&wire),
            ErrorKind::TrailingBytes { remaining: 1 }
        );
    }

    #[test]
    fn suite_list_too_long_to_encode_is_refused_not_truncated() {
        let suites = vec![CipherSuite::EcdheRsaWithAes128GcmSha256; 0x8000];
        let hello = ClientHello::new([0; 32], suites, Vec::new());
        assert!(hello.to_bytes().is_err());
    }

    #[test]
    fn certificate_running_past_its_list_is_an_error() {
        // The list is 4 bytes; its certificate claims 2 bytes after a 3-byte length, and only
        // 1 is inside the list.
        let wire = [0, 0, 4, 0, 0, 2, 0xaa, 0xbb];
        assert_eq!(
            Certificate::decode_exact(&wire).unwrap_err().kind,
            ErrorKind::UnexpectedEof {
                needed: 8,
                remaining: 0
            }
        );
    }

    #[test]
    fn certificate_list_longer_than_the_message_is_an_error() {
        let wire = [0, 0, 9, 0, 0, 2, 0xaa, 0xbb];
        assert!(Certificate::decode_exact(&wire).is_err());
    }

    #[test]
    fn finished_of_any_length_round_trips() {
        for wire in [vec![], vec![0xaa; 11], vec![0xbb; 36]] {
            let finished = Finished::decode_exact(&wire).unwrap();
            assert_eq!(finished.verify_data, wire);
            assert_eq!(finished.to_bytes().unwrap(), wire);
        }
    }

    /// Every strict prefix of a captured message's body, with its length.
    fn prefixes(message: &str) -> impl Iterator<Item = Vec<u8>> {
        let body = hex(message)[4..].to_vec();
        (0..body.len()).map(move |len| body[..len].to_vec())
    }

    #[test]
    fn truncated_messages_with_a_fixed_end_are_errors() {
        for body in prefixes(support::CERTIFICATE) {
            assert!(Certificate::decode_exact(&body).is_err(), "{}", body.len());
        }
        for body in prefixes(support::SERVER_KEY_EXCHANGE) {
            let result = ServerKeyExchange::decode_exact(&body);
            assert!(result.is_err(), "{}", body.len());
        }
        for body in prefixes(support::CLIENT_KEY_EXCHANGE) {
            let result = ClientKeyExchange::decode_exact(&body);
            assert!(result.is_err(), "{}", body.len());
        }
    }

    /// A hello cut exactly where its extension block would start is a valid older-style
    /// hello, so that one truncation decodes, with no extension block. Every other one fails.
    #[test]
    fn truncated_hellos_fail_except_at_the_extension_boundary() {
        let client: Vec<_> = prefixes(support::CLIENT_HELLO)
            .filter_map(|body| {
                ClientHello::decode_exact(&body)
                    .ok()
                    .map(|hello| (body, hello))
            })
            .collect();
        let [(body, hello)] = client.as_slice() else {
            panic!("{} truncations of the client hello decoded", client.len());
        };
        // version, random, empty session id, 46 bytes of suites, one compression method.
        assert_eq!(body.len(), 2 + 32 + 1 + (2 + 46) + (1 + 1));
        assert_eq!(hello.extensions, None);

        let server: Vec<_> = prefixes(support::SERVER_HELLO)
            .filter_map(|body| {
                ServerHello::decode_exact(&body)
                    .ok()
                    .map(|hello| (body, hello))
            })
            .collect();
        let [(body, hello)] = server.as_slice() else {
            panic!("{} truncations of the server hello decoded", server.len());
        };
        // version, random, empty session id, the suite, the compression method.
        assert_eq!(body.len(), 2 + 32 + 1 + 2 + 1);
        assert_eq!(hello.extensions, None);
    }
}
