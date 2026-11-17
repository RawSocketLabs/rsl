//! Golden wire vectors. Two kinds: bytes laid out by hand from the RFC's structure
//! definitions, and one real handshake captured from OpenSSL (`support`), which every
//! message type must decode and re-encode byte for byte.

mod support;

mod contract {
    use bnb::u24;
    use crypto::digest::sha2::sha256::Sha256;
    use tls::{
        PeerChain,
        record::{Aes128GcmProtection, TrafficKeys},
        schedule::{ConnectionEnd, MasterSecret},
        wire::{
            Alert, AlertDescription, AlertLevel, Asn1Cert, Certificate, CipherSuite, ClientHello,
            ClientKeyExchange, ContentType, EcCurveType, Extension, ExtensionType, Finished,
            HandshakeHeader, HandshakeType, NamedGroup, ProtocolVersion, RecordHeader, ServerHello,
            ServerKeyExchange, SignatureScheme,
        },
    };

    use super::support::{self, hex};

    /// Splits a captured handshake message into its header and body, checking that the
    /// header's length is the body's length.
    fn split(message: &str, expected: HandshakeType) -> Vec<u8> {
        let bytes = hex(message);
        let (header, used) = HandshakeHeader::decode_prefix(&bytes).unwrap();
        assert_eq!(used, HandshakeHeader::LEN);
        assert_eq!(header.msg_type, expected);
        let body = bytes[used..].to_vec();
        assert_eq!(header.length, u24::try_from(body.len()).unwrap());
        assert_eq!(header.to_bytes().unwrap(), bytes[..used]);
        body
    }

    // --- RFC 5246 §6.2.1: one type byte, two version bytes, a big-endian uint16 length ---

    #[test]
    fn record_header_is_type_then_version_then_big_endian_length() {
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
    fn encoded_record_header_is_len_bytes() {
        let header = RecordHeader::new(ContentType::Handshake, ProtocolVersion::TLS_1_2, 0);
        assert_eq!(header.to_bytes().unwrap().len(), RecordHeader::LEN);
    }

    // --- RFC 5246 §7.4: one type byte, then a big-endian uint24 body length ---

    #[test]
    fn handshake_header_is_type_then_big_endian_uint24_length() {
        let header = HandshakeHeader::new(HandshakeType::Finished, u24::new(0x01_02_03));
        let wire = [20, 0x01, 0x02, 0x03];
        assert_eq!(header.to_bytes().unwrap(), wire);
        assert_eq!(HandshakeHeader::decode_exact(&wire).unwrap(), header);
    }

    // --- RFC 5246 §7.2: a level byte, then a description byte ---

    #[test]
    fn alert_is_level_then_description() {
        let alert = Alert::new(AlertLevel::Fatal, AlertDescription::HandshakeFailure);
        assert_eq!(alert.to_bytes().unwrap(), [2, 40]);
        assert_eq!(Alert::decode_exact(&[2, 40]).unwrap(), alert);
    }

    // --- RFC 5246 §7.4.2: uint24 list length, then each certificate behind a uint24 length ---

    #[test]
    fn certificate_list_nests_uint24_lengths() {
        let message = Certificate {
            certificate_list: vec![
                Asn1Cert {
                    der: vec![0xaa, 0xbb],
                },
                Asn1Cert { der: vec![0xcc] },
            ],
        };
        let wire = [0, 0, 9, 0, 0, 2, 0xaa, 0xbb, 0, 0, 1, 0xcc];
        assert_eq!(message.to_bytes().unwrap(), wire);
        assert_eq!(Certificate::decode_exact(&wire).unwrap(), message);
        assert_eq!(
            PeerChain::from(message).der(),
            [vec![0xaa, 0xbb], vec![0xcc]]
        );
    }

    // --- RFC 5246 §7.4.1.2: the extension block is absent, not empty, when no bytes follow ---

    /// A hello with one suite and no extension block, laid out field by field.
    fn minimal_client_hello_wire() -> Vec<u8> {
        let mut wire = vec![3, 3];
        wire.extend([0x11; 32]);
        wire.extend([0]); // session_id: empty
        wire.extend([0, 2, 0xc0, 0x2f]); // cipher_suites: two bytes, one suite
        wire.extend([1, 0]); // compression_methods: one byte, null
        wire
    }

    #[test]
    fn client_hello_without_an_extension_block_round_trips_as_absent() {
        let wire = minimal_client_hello_wire();
        let hello = ClientHello::decode_exact(&wire).unwrap();
        assert_eq!(hello.client_version, ProtocolVersion::TLS_1_2);
        assert_eq!(hello.random, [0x11; 32]);
        assert_eq!(hello.session_id, []);
        assert_eq!(
            hello.cipher_suites,
            [CipherSuite::EcdheRsaWithAes128GcmSha256]
        );
        assert_eq!(hello.compression_methods, [ClientHello::COMPRESSION_NULL]);
        assert_eq!(hello.extensions, None);
        assert_eq!(hello.to_bytes().unwrap(), wire);
    }

    #[test]
    fn client_hello_with_an_empty_extension_block_round_trips_as_empty() {
        let mut wire = minimal_client_hello_wire();
        wire.extend([0, 0]);
        let hello = ClientHello::decode_exact(&wire).unwrap();
        assert_eq!(hello.extensions, Some(Vec::new()));
        assert_eq!(hello.to_bytes().unwrap(), wire);
    }

    #[test]
    fn new_client_hello_is_tls_1_2_with_null_compression_and_no_session() {
        let suites = vec![CipherSuite::EcdheRsaWithAes128GcmSha256];
        let hello = ClientHello::new([0x11; 32], suites, Vec::new());
        let mut wire = minimal_client_hello_wire();
        wire.extend([0, 0]);
        assert_eq!(hello.to_bytes().unwrap(), wire);
    }

    // --- RFC 5246 §7.4.1.3: a session id is one length byte, then up to 32 bytes ---

    #[test]
    fn server_hello_carries_its_session_id_behind_one_length_byte() {
        let mut wire = vec![3, 3];
        wire.extend([0x22; 32]);
        wire.extend([4, 0xde, 0xad, 0xbe, 0xef]); // session_id
        wire.extend([0xc0, 0x2f]); // cipher_suite
        wire.extend([0]); // compression_method
        let hello = ServerHello::decode_exact(&wire).unwrap();
        assert_eq!(hello.random, [0x22; 32]);
        assert_eq!(hello.session_id, [0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(hello.cipher_suite, CipherSuite::EcdheRsaWithAes128GcmSha256);
        assert_eq!(hello.compression_method, 0);
        assert_eq!(hello.extensions, None);
        assert_eq!(hello.to_bytes().unwrap(), wire);
    }

    // --- Registry values the capture does not exercise, from the clauses that define them ---

    #[test]
    fn handshake_types_have_their_rfc_5246_values() {
        let cases = [
            (HandshakeType::HelloRequest, 0),
            (HandshakeType::ClientHello, 1),
            (HandshakeType::ServerHello, 2),
            (HandshakeType::Certificate, 11),
            (HandshakeType::ServerKeyExchange, 12),
            (HandshakeType::CertificateRequest, 13),
            (HandshakeType::ServerHelloDone, 14),
            (HandshakeType::CertificateVerify, 15),
            (HandshakeType::ClientKeyExchange, 16),
            (HandshakeType::Finished, 20),
        ];
        for (msg_type, value) in cases {
            let header = HandshakeHeader::new(msg_type, u24::new(0));
            assert_eq!(header.to_bytes().unwrap(), [value, 0, 0, 0], "{msg_type:?}");
        }
    }

    #[test]
    fn named_groups_have_their_rfc_8422_values() {
        let groups = [
            NamedGroup::Secp256r1,
            NamedGroup::Secp384r1,
            NamedGroup::Secp521r1,
            NamedGroup::X25519,
            NamedGroup::X448,
        ];
        let extension = Extension::supported_groups(&groups).unwrap();
        assert_eq!(
            extension.data,
            [0, 10, 0, 23, 0, 24, 0, 25, 0, 29, 0, 30],
            "a 10-byte list of five uint16 groups"
        );
    }

    // --- The OpenSSL handshake ---

    #[test]
    fn openssl_client_hello_decodes_and_re_encodes_identically() {
        let body = split(support::CLIENT_HELLO, HandshakeType::ClientHello);
        let hello = ClientHello::decode_exact(&body).unwrap();
        assert_eq!(hello.client_version, ProtocolVersion::TLS_1_2);
        assert_eq!(
            hello.random.to_vec(),
            hex("38d5f8bf80d79d0977cee305230c5e74044962b2a0253b27555382760de8cb2a")
        );
        assert_eq!(hello.session_id, []);
        assert_eq!(hello.cipher_suites.len(), 23);
        assert_eq!(
            hello.cipher_suites[..3],
            [
                CipherSuite::Other(0xc030),
                CipherSuite::Other(0xcca8),
                CipherSuite::EcdheRsaWithAes128GcmSha256,
            ]
        );
        assert_eq!(hello.compression_methods, [ClientHello::COMPRESSION_NULL]);
        assert_eq!(hello.to_bytes().unwrap(), body);
    }

    /// OpenSSL built these extensions itself, so matching them checks each constructor's
    /// encoding against an independent implementation.
    #[test]
    fn extension_constructors_match_what_openssl_sent() {
        let body = split(support::CLIENT_HELLO, HandshakeType::ClientHello);
        let hello = ClientHello::decode_exact(&body).unwrap();
        let expected = vec![
            Extension::renegotiation_info_initial(),
            Extension::ec_point_formats_uncompressed(),
            Extension::supported_groups(&[NamedGroup::X25519]).unwrap(),
            Extension::new(ExtensionType::Other(35), Vec::new()), // session_ticket
            Extension::new(ExtensionType::Other(22), Vec::new()), // encrypt_then_mac
            Extension::extended_master_secret(),
            Extension::signature_algorithms(&[SignatureScheme::RsaPssRsaeSha256]).unwrap(),
            Extension::server_name("tls.test").unwrap(),
        ];
        assert_eq!(hello.extensions, Some(expected));
    }

    #[test]
    fn openssl_server_hello_decodes_and_re_encodes_identically() {
        let body = split(support::SERVER_HELLO, HandshakeType::ServerHello);
        let hello = ServerHello::decode_exact(&body).unwrap();
        assert_eq!(hello.server_version, ProtocolVersion::TLS_1_2);
        assert_eq!(
            hello.random.to_vec(),
            hex("3ffd170ee88750c48367d606764aa89df97a6874b52f6e00ceac8a2fd20b41af")
        );
        assert_eq!(
            hello.session_id,
            hex("e52c1773269b222179c43fcf35d7c7a7ca2cd85bd52207dd65266b5b6fc9962f")
        );
        assert_eq!(hello.cipher_suite, CipherSuite::EcdheRsaWithAes128GcmSha256);
        assert_eq!(hello.compression_method, 0);
        assert_eq!(
            hello.extension(ExtensionType::RenegotiationInfo),
            Some(&Extension::renegotiation_info_initial())
        );
        assert_eq!(
            hello.extension(ExtensionType::ExtendedMasterSecret),
            Some(&Extension::extended_master_secret())
        );
        assert_eq!(hello.extension(ExtensionType::ServerName), None);
        assert_eq!(hello.to_bytes().unwrap(), body);
    }

    #[test]
    fn openssl_certificate_decodes_and_re_encodes_identically() {
        let body = split(support::CERTIFICATE, HandshakeType::Certificate);
        let message = Certificate::decode_exact(&body).unwrap();
        assert_eq!(message.certificate_list.len(), 1);
        assert_eq!(message.certificate_list[0].der.len(), 767);
        assert_eq!(message.to_bytes().unwrap(), body);
    }

    #[test]
    fn openssl_server_key_exchange_decodes_and_re_encodes_identically() {
        let body = split(
            support::SERVER_KEY_EXCHANGE,
            HandshakeType::ServerKeyExchange,
        );
        let exchange = ServerKeyExchange::decode_exact(&body).unwrap();
        assert_eq!(exchange.params.curve_type, EcCurveType::NamedCurve);
        assert_eq!(exchange.params.named_group, NamedGroup::X25519);
        assert_eq!(
            exchange.params.public,
            hex("72a309e870a4be2798ad55cd67f633760faa8c8394d8d18f4b0af7f5d81d9546")
        );
        assert_eq!(exchange.algorithm, SignatureScheme::RsaPssRsaeSha256);
        assert_eq!(exchange.signature.len(), 256);
        assert_eq!(exchange.to_bytes().unwrap(), body);
    }

    #[test]
    fn openssl_server_hello_done_has_an_empty_body() {
        let body = split(support::SERVER_HELLO_DONE, HandshakeType::ServerHelloDone);
        assert_eq!(body, []);
    }

    #[test]
    fn openssl_client_key_exchange_decodes_and_re_encodes_identically() {
        let body = split(
            support::CLIENT_KEY_EXCHANGE,
            HandshakeType::ClientKeyExchange,
        );
        let exchange = ClientKeyExchange::decode_exact(&body).unwrap();
        assert_eq!(
            exchange.public,
            hex("45084ea0f1f3cd998b579c3ff76b833c23ca5382e2cf9e77db4aff1179f73a3a")
        );
        assert_eq!(exchange.to_bytes().unwrap(), body);
    }

    #[test]
    fn openssl_finished_decodes_and_re_encodes_identically() {
        let body = split(support::CLIENT_FINISHED, HandshakeType::Finished);
        let finished = Finished::decode_exact(&body).unwrap();
        assert_eq!(finished.verify_data, hex("52f5a48b7625e4f9293bccb2"));
        assert_eq!(finished.verify_data.len(), Finished::VERIFY_DATA_LEN);
        assert_eq!(finished.to_bytes().unwrap(), body);
    }

    // --- The key schedule ---

    /// The capture's two hello randoms.
    fn randoms() -> ([u8; 32], [u8; 32]) {
        let client = ClientHello::decode_exact(&hex(support::CLIENT_HELLO)[4..]).unwrap();
        let server = ServerHello::decode_exact(&hex(support::SERVER_HELLO)[4..]).unwrap();
        (client.random, server.random)
    }

    /// The capture's master secret, as OpenSSL logged it.
    fn master_secret() -> MasterSecret {
        MasterSecret::from_bytes(hex(support::MASTER_SECRET).try_into().unwrap())
    }

    /// SHA-256 over the capture's handshake messages, in order, up to `last`.
    fn handshake_hash(last: &str) -> [u8; 32] {
        let messages = [
            support::CLIENT_HELLO,
            support::SERVER_HELLO,
            support::CERTIFICATE,
            support::SERVER_KEY_EXCHANGE,
            support::SERVER_HELLO_DONE,
            support::CLIENT_KEY_EXCHANGE,
            support::CLIENT_FINISHED,
        ];
        let mut hash = Sha256::new();
        for message in messages {
            hash.update(hex(message)).unwrap();
            if message == last {
                return hash.finalize().into_bytes();
            }
        }
        panic!("{last} is not one of the capture's messages");
    }

    /// `openssl kdf -keylen 48 -kdfopt digest:SHA256 -kdfopt hexsecret:0001…1f -kdfopt
    /// hexseed:<"master secret", then 32 bytes of aa, then 32 bytes of bb> TLS1-PRF`
    /// (OpenSSL 4.0.3, 2026-11-16). The two randoms differ so that their order matters.
    #[test]
    fn master_secret_is_the_prf_over_client_then_server_random() {
        let pre_master: Vec<u8> = (0x00..0x20).collect();
        let secret = MasterSecret::derive(&pre_master, &[0xaa; 32], &[0xbb; 32]).unwrap();
        assert_eq!(
            secret.expose_secret().to_vec(),
            hex(concat!(
                "f2a68ef2ee09028e513da57819eb1fe62c547e2dea9a310b9aef1b47c9548c00",
                "b66941443893edd6349929cacb120b51",
            ))
        );
    }

    /// As above with `hexseed:<"extended master secret", then 2021…3f>`.
    #[test]
    fn extended_master_secret_is_the_prf_over_the_session_hash() {
        let pre_master: Vec<u8> = (0x00..0x20).collect();
        let session_hash: [u8; 32] = hex(concat!(
            "202122232425262728292a2b2c2d2e2f",
            "303132333435363738393a3b3c3d3e3f",
        ))
        .try_into()
        .unwrap();
        let secret = MasterSecret::derive_extended(&pre_master, &session_hash).unwrap();
        assert_eq!(
            secret.expose_secret().to_vec(),
            hex(concat!(
                "913e8eae1e983c8da7ac4ba74bd309343125600986d88b3e13d195d5338edc45",
                "22afe385510c28f45a9087c97fe937cb",
            ))
        );
    }

    #[test]
    fn finished_check_values_match_the_ones_openssl_sent() {
        let secret = master_secret();
        let client = Finished::decode_exact(&hex(support::CLIENT_FINISHED)[4..]).unwrap();
        let server = Finished::decode_exact(&hex(support::SERVER_FINISHED)[4..]).unwrap();
        // The client's covers everything before it; the server's also covers the client's.
        let before_client = handshake_hash(support::CLIENT_KEY_EXCHANGE);
        let before_server = handshake_hash(support::CLIENT_FINISHED);
        assert_eq!(
            secret
                .finished_verify_data(ConnectionEnd::Client, &before_client)
                .unwrap()
                .to_vec(),
            client.verify_data
        );
        assert_eq!(
            secret
                .finished_verify_data(ConnectionEnd::Server, &before_server)
                .unwrap()
                .to_vec(),
            server.verify_data
        );
    }

    // --- Record protection ---

    /// Splits a captured protected record into its header and fragment.
    fn record(record: &str, expected: ContentType) -> (RecordHeader, Vec<u8>) {
        let bytes = hex(record);
        let (header, used) = RecordHeader::decode_prefix(&bytes).unwrap();
        assert_eq!(header.content_type, expected);
        assert_eq!(usize::from(header.length), bytes.len() - used);
        (header, bytes[used..].to_vec())
    }

    /// The capture's record keys, derived from its master secret and randoms.
    fn traffic_keys() -> TrafficKeys {
        let (client_random, server_random) = randoms();
        master_secret()
            .aes128_gcm_keys(&client_random, &server_random)
            .unwrap()
    }

    #[test]
    fn records_openssl_sent_open_to_what_it_sent_in_them() {
        let keys = traffic_keys();
        let version = ProtocolVersion::TLS_1_2;

        let (header, fragment) = record(support::CLIENT_FINISHED_RECORD, ContentType::Handshake);
        assert_eq!(header.version, version);
        assert_eq!(
            keys.client_write
                .open(0, ContentType::Handshake, version, &fragment),
            Ok(hex(support::CLIENT_FINISHED))
        );

        let (_, fragment) = record(
            support::CLIENT_APPLICATION_RECORD,
            ContentType::ApplicationData,
        );
        assert_eq!(
            keys.client_write
                .open(1, ContentType::ApplicationData, version, &fragment),
            Ok(b"GET / HTTP/1.0\r\n\r\n".to_vec())
        );

        let (_, fragment) = record(support::CLIENT_CLOSE_RECORD, ContentType::Alert);
        let alert = keys
            .client_write
            .open(2, ContentType::Alert, version, &fragment)
            .unwrap();
        assert_eq!(
            Alert::decode_exact(&alert).unwrap(),
            Alert::new(AlertLevel::Warning, AlertDescription::CloseNotify)
        );

        let (_, fragment) = record(support::SERVER_FINISHED_RECORD, ContentType::Handshake);
        assert_eq!(
            keys.server_write
                .open(0, ContentType::Handshake, version, &fragment),
            Ok(hex(support::SERVER_FINISHED))
        );
    }

    #[test]
    fn sealing_with_openssls_explicit_nonce_reproduces_its_record() {
        let keys = traffic_keys();
        let (_, fragment) = record(
            support::CLIENT_APPLICATION_RECORD,
            ContentType::ApplicationData,
        );
        let explicit_nonce = fragment[..8].try_into().unwrap();
        let sealed = keys
            .client_write
            .seal_with_explicit_nonce(
                explicit_nonce,
                1,
                ContentType::ApplicationData,
                ProtocolVersion::TLS_1_2,
                b"GET / HTTP/1.0\r\n\r\n",
            )
            .unwrap();
        assert_eq!(sealed, fragment);
    }

    #[test]
    fn seal_puts_the_sequence_number_in_front_as_the_explicit_nonce() {
        let keys = traffic_keys();
        let version = ProtocolVersion::TLS_1_2;
        let sequence_number = 0x0102_0304_0506_0708;
        let sealed = keys
            .client_write
            .seal(
                sequence_number,
                ContentType::ApplicationData,
                version,
                b"hello",
            )
            .unwrap();
        assert_eq!(sealed[..8], [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(sealed.len(), 5 + Aes128GcmProtection::OVERHEAD);
        assert_eq!(
            keys.client_write.open(
                sequence_number,
                ContentType::ApplicationData,
                version,
                &sealed
            ),
            Ok(b"hello".to_vec())
        );
    }
}
