//! Adversarial decoding: every byte here comes from the network, so hostile or truncated
//! input must be a clean error or a preserved value, never a panic and never a silent
//! misreading of where a list ends.

mod support;

mod adversarial {
    use bnb::ErrorKind;
    use tls::{
        handshake::{HandshakeJoiner, MessageTooLong},
        record::{Aes128GcmProtection, ReadState, RecordError, WriteState},
        schedule::MasterSecret,
        wire::{
            Certificate, CipherSuite, ClientHello, ClientKeyExchange, ContentType, Extension,
            ExtensionType, Finished, HandshakeType, ProtocolVersion, RecordHeader, ServerHello,
            ServerKeyExchange,
        },
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
        // version, random, a 32-byte session id, the suite, the compression method.
        assert_eq!(body.len(), 2 + 32 + (1 + 32) + 2 + 1);
        assert_eq!(hello.extensions, None);
    }

    // --- Record protection ---

    /// The capture's client-side record protection and its application-data fragment.
    fn client_application_record() -> (Aes128GcmProtection, Vec<u8>) {
        let client = ClientHello::decode_exact(&hex(support::CLIENT_HELLO)[4..]).unwrap();
        let server = ServerHello::decode_exact(&hex(support::SERVER_HELLO)[4..]).unwrap();
        let secret = MasterSecret::from_bytes(hex(support::MASTER_SECRET).try_into().unwrap());
        let keys = secret
            .aes128_gcm_keys(&client.random, &server.random)
            .unwrap();
        let fragment = hex(support::CLIENT_APPLICATION_RECORD)[RecordHeader::LEN..].to_vec();
        (keys.client_write, fragment)
    }

    /// Opens `fragment` as the capture's second client record unless told otherwise.
    fn open(protection: &Aes128GcmProtection, fragment: &[u8]) -> Result<Vec<u8>, RecordError> {
        let version = ProtocolVersion::TLS_1_2;
        protection.open(1, ContentType::ApplicationData, version, fragment)
    }

    #[test]
    fn record_with_any_flipped_bit_fails_authentication() {
        let (protection, fragment) = client_application_record();
        assert!(open(&protection, &fragment).is_ok());
        for bit in 0..fragment.len() * 8 {
            let mut altered = fragment.clone();
            altered[bit / 8] ^= 1 << (bit % 8);
            assert_eq!(
                open(&protection, &altered),
                Err(RecordError::BadRecordMac),
                "bit {bit}"
            );
        }
    }

    #[test]
    fn record_opened_as_another_sequence_type_or_version_fails_authentication() {
        let (protection, fragment) = client_application_record();
        let version = ProtocolVersion::TLS_1_2;
        let data = ContentType::ApplicationData;
        assert_eq!(
            protection.open(2, data, version, &fragment),
            Err(RecordError::BadRecordMac),
            "replayed at another sequence number"
        );
        assert_eq!(
            protection.open(1, ContentType::Handshake, version, &fragment),
            Err(RecordError::BadRecordMac),
            "relabelled as another content type"
        );
        assert_eq!(
            protection.open(1, data, ProtocolVersion::TLS_1_1, &fragment),
            Err(RecordError::BadRecordMac),
            "relabelled as another version"
        );
    }

    #[test]
    fn record_opened_with_the_other_directions_key_fails_authentication() {
        let (_, fragment) = client_application_record();
        let client = ClientHello::decode_exact(&hex(support::CLIENT_HELLO)[4..]).unwrap();
        let server = ServerHello::decode_exact(&hex(support::SERVER_HELLO)[4..]).unwrap();
        let secret = MasterSecret::from_bytes(hex(support::MASTER_SECRET).try_into().unwrap());
        let keys = secret
            .aes128_gcm_keys(&client.random, &server.random)
            .unwrap();
        assert_eq!(
            open(&keys.server_write, &fragment),
            Err(RecordError::BadRecordMac)
        );
    }

    #[test]
    fn record_shorter_than_its_nonce_and_tag_is_truncated_not_a_panic() {
        let (protection, fragment) = client_application_record();
        for len in 0..Aes128GcmProtection::OVERHEAD {
            assert_eq!(
                open(&protection, &fragment[..len]),
                Err(RecordError::Truncated),
                "{len} bytes"
            );
        }
        assert_eq!(
            open(&protection, &fragment[..Aes128GcmProtection::OVERHEAD]),
            Err(RecordError::BadRecordMac),
            "an empty record that is long enough still has to authenticate"
        );
    }

    #[test]
    fn plaintext_whose_record_would_not_fit_a_16_bit_length_is_refused() {
        let (protection, _) = client_application_record();
        let version = ProtocolVersion::TLS_1_2;
        let plaintext = vec![0; Aes128GcmProtection::MAX_PLAINTEXT_LEN + 1];
        let longest = &plaintext[..Aes128GcmProtection::MAX_PLAINTEXT_LEN];
        let sealed = protection
            .seal(0, ContentType::ApplicationData, version, longest)
            .unwrap();
        assert_eq!(sealed.len(), usize::from(u16::MAX));
        assert_eq!(
            protection.seal(0, ContentType::ApplicationData, version, &plaintext),
            Err(RecordError::PlaintextTooLong)
        );
    }

    // --- The stateful record layer ---

    /// A read state keyed to open what the capture's client sent.
    fn client_record_reader() -> ReadState {
        let (protection, _) = client_application_record();
        let mut state = ReadState::plain();
        state.protect(protection);
        state
    }

    #[test]
    fn record_delivered_out_of_order_fails_authentication() {
        let mut state = client_record_reader();
        // The capture's second record, offered where the first is expected.
        state.receive(&hex(support::CLIENT_APPLICATION_RECORD));
        assert_eq!(state.next_record(), Err(RecordError::BadRecordMac));
    }

    #[test]
    fn record_replayed_fails_authentication() {
        let mut state = client_record_reader();
        let first = hex(support::CLIENT_FINISHED_RECORD);
        state.receive(&first);
        assert!(state.next_record().unwrap().is_some());
        state.receive(&first);
        assert_eq!(state.next_record(), Err(RecordError::BadRecordMac));
    }

    #[test]
    fn oversized_record_is_refused_as_soon_as_its_header_arrives() {
        // 2^14 + 2048 + 1 bytes claimed, protected; 2^14 + 1 claimed, unprotected.
        let mut protected = client_record_reader();
        protected.receive(&[23, 3, 3, 0x48, 0x01]);
        assert_eq!(protected.next_record(), Err(RecordError::RecordOverflow));

        let mut plain = ReadState::plain();
        plain.receive(&[22, 3, 3, 0x40, 0x01]);
        assert_eq!(plain.next_record(), Err(RecordError::RecordOverflow));
    }

    #[test]
    fn largest_allowed_records_are_not_refused_by_their_headers() {
        let mut protected = client_record_reader();
        protected.receive(&[23, 3, 3, 0x48, 0x00]);
        assert_eq!(protected.next_record(), Ok(None));

        let mut plain = ReadState::plain();
        plain.receive(&[22, 3, 3, 0x40, 0x00]);
        assert_eq!(plain.next_record(), Ok(None));
    }

    #[test]
    fn read_buffer_never_holds_more_than_one_largest_record() {
        let mut state = client_record_reader();
        // A header announcing the largest protected record, then far more bytes than that.
        let mut flood = vec![23, 3, 3, 0x48, 0x00];
        flood.resize(ReadState::CAPACITY * 4, 0);
        assert_eq!(state.receive(&flood), ReadState::CAPACITY);
        assert_eq!(state.receive_limit(), 0);
        assert_eq!(state.receive(&flood), 0);
    }

    #[test]
    fn read_state_takes_bytes_only_to_the_end_of_one_record() {
        let mut state = client_record_reader();
        let first = hex(support::CLIENT_FINISHED_RECORD);
        let second = hex(support::CLIENT_APPLICATION_RECORD);
        let both = [first.as_slice(), &second].concat();
        assert_eq!(
            state.receive(&both),
            first.len(),
            "the second record is left"
        );
        assert_eq!(
            state.receive(&both[first.len()..]),
            0,
            "until the first is taken"
        );
        assert!(state.next_record().unwrap().is_some());
        assert_eq!(state.receive(&both[first.len()..]), second.len());
    }

    #[test]
    fn partial_record_is_reported_so_a_cut_connection_is_noticed() {
        let mut state = client_record_reader();
        let record = hex(support::CLIENT_FINISHED_RECORD);
        state.receive(&record[..record.len() - 1]);
        assert_eq!(state.next_record(), Ok(None));
        assert!(state.has_partial_record());
    }

    #[test]
    fn write_state_refuses_more_plaintext_than_one_record_may_carry() {
        let mut state = WriteState::plain();
        let version = ProtocolVersion::TLS_1_2;
        let most = vec![0; usize::from(RecordHeader::MAX_PLAINTEXT_LEN)];
        assert!(
            state
                .seal(ContentType::ApplicationData, version, &most)
                .is_ok()
        );
        let more = vec![0; usize::from(RecordHeader::MAX_PLAINTEXT_LEN) + 1];
        assert_eq!(
            state.seal(ContentType::ApplicationData, version, &more),
            Err(RecordError::PlaintextTooLong)
        );
    }

    // --- Handshake reassembly ---

    #[test]
    fn overlong_handshake_message_is_refused_as_soon_as_its_header_arrives() {
        let mut joiner = HandshakeJoiner::new();
        joiner.push(&[11, 0x01, 0x00, 0x01]); // a Certificate of 64 KiB + 1
        assert_eq!(
            joiner.next_message(),
            Err(MessageTooLong {
                msg_type: HandshakeType::Certificate,
                length: HandshakeJoiner::MAX_BODY_LEN + 1,
            })
        );
    }

    #[test]
    fn longest_allowed_handshake_message_waits_for_its_body() {
        let mut joiner = HandshakeJoiner::new();
        joiner.push(&[11, 0x01, 0x00, 0x00]);
        assert_eq!(joiner.next_message(), Ok(None));
        assert!(
            !joiner.is_empty(),
            "a partial message is visible at a key change"
        );
    }

    #[test]
    fn unknown_handshake_type_is_joined_and_left_for_the_caller_to_reject() {
        let mut joiner = HandshakeJoiner::new();
        joiner.push(&[0x7f, 0, 0, 1, 0xaa]);
        let message = joiner.next_message().unwrap().unwrap();
        assert_eq!(message.msg_type(), HandshakeType::Other(0x7f));
        assert_eq!(message.body(), [0xaa]);
        assert!(joiner.is_empty());
    }
}
