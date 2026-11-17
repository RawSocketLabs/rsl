//! The client state machine, against a recorded OpenSSL connection (`support`).
//!
//! The recording's server bytes came from OpenSSL, and the client's randomness in it is
//! reproducible, so replaying it checks the whole client against an independent
//! implementation. The rejection tests change that recording one thing at a time; for the
//! protected part of the connection they re-derive the session keys from the recording and
//! play the server themselves.

mod support;

mod client {
    use crypto::signature::ed25519::Ed25519SigningKey;
    use pki::issuance::Ed25519CertificateSigner;
    use tls::{
        CertificateError, Client, ClientConfig, Error, ServerVerification,
        handshake::{HandshakeMessage, Transcript},
        record::{Aes128GcmProtection, ReadState, RecordError, TrafficKeys, WriteState},
        schedule::{ConnectionEnd, MasterSecret},
        wire::{
            Alert, AlertDescription, AlertLevel, Asn1Cert, Certificate, ContentType, Extension,
            ExtensionType, HandshakeType, ProtocolVersion, ServerHello, ServerKeyExchange,
        },
    };
    use x509::{
        Time,
        builder::{CertificateBuilder, NameDer},
    };

    use super::support::{self, CountingRandom, ReplaySession, hex};

    const REQUEST: &[u8] = b"hello tls\n";
    const RESPONSE: &[u8] = b"slt olleh\n";
    const VERSION: ProtocolVersion = ProtocolVersion::TLS_1_2;

    // Positions of the recording's records.
    const SERVER_HELLO: usize = 0;
    const CERTIFICATE: usize = 1;
    const SERVER_KEY_EXCHANGE: usize = 2;
    const SERVER_HELLO_DONE: usize = 3;
    const SERVER_CCS: usize = 4;
    const SERVER_FINISHED: usize = 5;
    const SERVER_DATA: usize = 6;
    const SERVER_CLOSE: usize = 7;

    /// Splits a byte stream into whole records.
    fn records(wire: &[u8]) -> Vec<Vec<u8>> {
        let mut records = Vec::new();
        let mut rest = wire;
        while !rest.is_empty() {
            let end = 5 + usize::from(u16::from_be_bytes([rest[3], rest[4]]));
            records.push(rest[..end].to_vec());
            rest = &rest[end..];
        }
        records
    }

    /// One unprotected record carrying `fragment`.
    fn plain_record(content_type: ContentType, fragment: &[u8]) -> Vec<u8> {
        WriteState::plain()
            .seal(content_type, VERSION, fragment)
            .unwrap()
    }

    fn server_records() -> Vec<Vec<u8>> {
        records(&hex(support::REPLAY_SERVER))
    }

    fn client_records() -> Vec<Vec<u8>> {
        records(&hex(support::REPLAY_CLIENT))
    }

    /// A client seeded exactly as the one in the recording was.
    fn new_client() -> Client<'static> {
        let config = ClientConfig {
            server_name: Some("tls.test"),
            verification: ServerVerification::Unverified,
        };
        Client::new(config, &mut CountingRandom::default()).unwrap()
    }

    /// Takes everything the client has queued for the peer.
    fn drain(client: &mut Client<'_>) -> Vec<u8> {
        let bytes = client.transmit().to_vec();
        client.sent(bytes.len());
        bytes
    }

    /// Feeds whole records to the client, expecting each to be accepted in full.
    fn feed(client: &mut Client<'_>, records: &[Vec<u8>]) -> Result<(), Error> {
        for record in records {
            assert_eq!(client.receive(record)?, record.len());
        }
        Ok(())
    }

    /// A client that has sent its hello, and the recording's server records.
    fn started() -> (Client<'static>, Vec<Vec<u8>>) {
        let mut client = new_client();
        drain(&mut client);
        (client, server_records())
    }

    /// Asserts that what the client queued is exactly one unprotected fatal alert with this
    /// description, or nothing when `description` is `None`.
    #[track_caller]
    fn assert_alert_queued(client: &mut Client<'_>, description: Option<AlertDescription>) {
        let queued = records(&drain(client));
        let Some(description) = description else {
            assert_eq!(
                queued,
                Vec::<Vec<u8>>::new(),
                "no alert is owed to the peer"
            );
            return;
        };
        let alert = Alert::new(AlertLevel::Fatal, description);
        let [record] = queued.as_slice() else {
            panic!("expected one alert record, found {}", queued.len());
        };
        // The record says TLS 1.0 until a ServerHello has been accepted, TLS 1.2 after.
        assert_eq!((record[0], record[1]), (21, 3), "an alert record");
        assert_eq!(record[5..], alert.to_bytes().unwrap(), "the alert");
    }

    /// Asserts that feeding `records` to a started client fails with `expected`, that the
    /// client never becomes established or yields plaintext, that it queues exactly the
    /// alert `expected` calls for, and that it refuses further input.
    #[track_caller]
    fn assert_rejected(records: &[Vec<u8>], expected: &Error) {
        let (mut client, _) = started();
        let error = feed(&mut client, records).unwrap_err();
        assert_eq!(&error, expected);
        assert!(!client.is_established());
        assert_eq!(client.plaintext(), []);
        assert_alert_queued(&mut client, expected.alert());
        assert_eq!(client.receive(&[0]), Err(Error::InvalidState));
    }

    /// Rebuilds the recording's `ServerHello` record after `change` edits the message.
    fn server_hello_with(change: impl FnOnce(&mut ServerHello)) -> Vec<u8> {
        let record = &server_records()[SERVER_HELLO];
        let mut hello = ServerHello::decode_exact(&record[9..]).unwrap();
        change(&mut hello);
        let message = HandshakeMessage::new(HandshakeType::ServerHello, &hello.to_bytes().unwrap());
        plain_record(ContentType::Handshake, message.bytes())
    }

    // --- Replay ---

    /// Plays the recording's exchange, feeding the server's bytes `step` at a time, and
    /// returns everything the client sent and everything it received as plaintext.
    fn replay(step: usize) -> (Vec<u8>, Vec<u8>) {
        let server = server_records();
        let mut client = new_client();
        let (mut sent, mut received) = (Vec::new(), Vec::new());
        let mut exchange = |client: &mut Client<'_>, records: &[Vec<u8>]| {
            sent.extend(drain(client));
            for piece in records.concat().chunks(step) {
                assert_eq!(client.receive(piece), Ok(piece.len()));
            }
            received.extend(client.plaintext());
            client.consume(usize::MAX);
        };

        exchange(&mut client, &server[..=SERVER_HELLO_DONE]);
        assert!(!client.is_established());
        exchange(&mut client, &server[SERVER_CCS..=SERVER_FINISHED]);
        assert!(client.is_established());
        assert_eq!(client.write(REQUEST), Ok(REQUEST.len()));
        exchange(&mut client, &server[SERVER_DATA..=SERVER_DATA]);
        client.close().unwrap();
        assert!(!client.peer_closed());
        exchange(&mut client, &server[SERVER_CLOSE..]);
        assert!(client.peer_closed());
        assert_eq!(client.receive_eof(), Ok(()));
        sent.extend(drain(&mut client));
        (sent, received)
    }

    #[test]
    fn replaying_openssls_bytes_reproduces_the_recorded_connection() {
        for step in [usize::MAX, 1, 7] {
            let (sent, received) = replay(step);
            // Independent: the client accepted OpenSSL's signature, Finished, data and close.
            assert_eq!(received, RESPONSE, "{step}");
            // Golden (characterization): the client's own bytes, which OpenSSL accepted when
            // they were recorded. A change here needs the interop test and a new recording.
            assert_eq!(sent, hex(support::REPLAY_CLIENT), "{step}");
        }
    }

    #[test]
    fn handshake_reports_the_servers_session_id_and_chain() {
        let (mut client, server) = started();
        assert_eq!(client.session_id(), None);
        assert!(client.peer_chain().is_none());
        feed(&mut client, &server[..=CERTIFICATE]).unwrap();
        let hello = ServerHello::decode_exact(&server[SERVER_HELLO][9..]).unwrap();
        assert_eq!(client.session_id(), Some(hello.session_id.as_slice()));
        // The record is a 5-byte header, a 4-byte message header, then 3 + 3 length bytes.
        let der = server[CERTIFICATE][15..].to_vec();
        assert_eq!(client.peer_chain().unwrap().der(), [der]);
    }

    #[test]
    fn server_flight_is_accepted_however_it_is_split_into_records() {
        let (_, server) = started();
        let messages: Vec<u8> = server[..=SERVER_HELLO_DONE]
            .iter()
            .flat_map(|record| record[5..].to_vec())
            .collect();
        for size in [messages.len(), 3] {
            let mut client = new_client();
            drain(&mut client);
            for fragment in messages.chunks(size) {
                feed(
                    &mut client,
                    &[plain_record(ContentType::Handshake, fragment)],
                )
                .unwrap();
            }
            let flight = client_records()[1..=3].concat();
            assert_eq!(drain(&mut client), flight, "{size}");
        }
    }

    #[test]
    fn data_in_the_same_flight_as_finished_is_delivered() {
        let (mut client, server) = started();
        feed(&mut client, &server[..=SERVER_HELLO_DONE]).unwrap();
        let flight = server[SERVER_CCS..=SERVER_DATA].concat();
        assert_eq!(client.receive(&flight), Ok(flight.len()));
        assert!(client.is_established());
        assert_eq!(client.plaintext(), RESPONSE);
    }

    // --- The server's first flight, changed one thing at a time ---

    #[test]
    fn messages_out_of_order_are_rejected() {
        let (_, server) = started();
        let out_of_order = Error::UnexpectedMessage("handshake message out of order");
        let pick = |order: &[usize]| -> Vec<Vec<u8>> {
            order.iter().map(|&at| server[at].clone()).collect()
        };
        let cases: [(&str, Vec<Vec<u8>>); 6] = [
            ("certificate first", pick(&[CERTIFICATE])),
            ("server hello twice", pick(&[SERVER_HELLO, SERVER_HELLO])),
            (
                "certificate dropped",
                pick(&[SERVER_HELLO, SERVER_KEY_EXCHANGE]),
            ),
            (
                "key exchange dropped",
                pick(&[SERVER_HELLO, CERTIFICATE, SERVER_HELLO_DONE]),
            ),
            (
                "certificate twice",
                pick(&[SERVER_HELLO, CERTIFICATE, CERTIFICATE]),
            ),
            (
                "hello done twice",
                pick(&[
                    SERVER_HELLO,
                    CERTIFICATE,
                    SERVER_KEY_EXCHANGE,
                    SERVER_HELLO_DONE,
                    SERVER_HELLO_DONE,
                ]),
            ),
        ];
        for (name, records) in cases {
            let (mut client, _) = started();
            let error = feed(&mut client, &records).unwrap_err();
            assert_eq!(error, out_of_order, "{name}");
            assert!(!client.is_established(), "{name}");
        }
        assert_rejected(&pick(&[CERTIFICATE]), &out_of_order);
    }

    #[test]
    fn change_cipher_spec_before_our_finished_is_rejected() {
        let (_, server) = started();
        let early = Error::UnexpectedMessage("ChangeCipherSpec");
        assert_rejected(&[server[SERVER_CCS].clone()], &early);
        assert_rejected(
            &[server[SERVER_HELLO].clone(), server[SERVER_CCS].clone()],
            &early,
        );
    }

    #[test]
    fn application_data_during_the_handshake_is_rejected() {
        let (_, server) = started();
        let data = plain_record(ContentType::ApplicationData, b"early");
        assert_rejected(
            &[server[SERVER_HELLO].clone(), data],
            &Error::UnexpectedMessage("application data during the handshake"),
        );
    }

    #[test]
    fn alert_during_the_handshake_ends_it_without_a_reply() {
        for (level, description) in [
            (AlertLevel::Fatal, AlertDescription::HandshakeFailure),
            (AlertLevel::Warning, AlertDescription::CloseNotify),
            (AlertLevel::Warning, AlertDescription::UserCanceled),
        ] {
            let alert = Alert::new(level, description);
            let record = plain_record(ContentType::Alert, &alert.to_bytes().unwrap());
            assert_rejected(&[record], &Error::PeerAlert(alert));
        }
    }

    #[test]
    fn malformed_records_are_rejected() {
        let (_, server) = started();
        assert_rejected(
            &[plain_record(ContentType::Handshake, &[])],
            &Error::UnexpectedMessage("empty handshake record"),
        );
        assert_rejected(
            &[plain_record(ContentType::Other(99), b"x")],
            &Error::UnexpectedMessage("unknown record type"),
        );
        // Half a message, then a record of another type.
        let half = plain_record(ContentType::Handshake, &server[SERVER_HELLO][5..20]);
        let alert = plain_record(ContentType::Alert, &[1, 0]);
        assert_rejected(
            &[half, alert],
            &Error::UnexpectedMessage("record interleaved with a handshake message"),
        );
        // A Certificate announcing 64 KiB + 1 bytes.
        let long = plain_record(ContentType::Handshake, &[11, 0x01, 0x00, 0x01]);
        let (mut client, _) = started();
        let error = feed(&mut client, &[server[SERVER_HELLO].clone(), long]).unwrap_err();
        assert!(matches!(error, Error::MessageTooLong(_)), "{error:?}");
        assert_alert_queued(&mut client, Some(AlertDescription::DecodeError));
    }

    #[test]
    fn record_version_other_than_tls_1_2_after_the_server_hello_is_rejected() {
        let (_, server) = started();
        let mut certificate = server[CERTIFICATE].clone();
        certificate[2] = 1;
        assert_rejected(
            &[server[SERVER_HELLO].clone(), certificate],
            &Error::ProtocolVersion,
        );
    }

    #[test]
    fn record_claiming_another_major_version_is_rejected_before_the_server_hello() {
        let (_, server) = started();
        let mut hello = server[SERVER_HELLO].clone();
        hello[1] = 2;
        assert_rejected(&[hello], &Error::ProtocolVersion);
    }

    #[test]
    fn server_hello_done_with_a_body_is_rejected() {
        let (_, server) = started();
        let message = HandshakeMessage::new(HandshakeType::ServerHelloDone, &[0]);
        let mut flight = server[..SERVER_HELLO_DONE].to_vec();
        flight.push(plain_record(ContentType::Handshake, message.bytes()));
        assert_rejected(&flight, &Error::IllegalParameter("ServerHelloDone body"));
    }

    #[test]
    fn server_hello_choosing_what_was_not_offered_is_rejected() {
        use ExtensionType::{ExtendedMasterSecret, RenegotiationInfo};
        let cases: [(Vec<u8>, Error); 8] = [
            (
                server_hello_with(|hello| hello.server_version = ProtocolVersion::TLS_1_1),
                Error::ProtocolVersion,
            ),
            (
                server_hello_with(|hello| hello.cipher_suite = 0x009c.into()),
                Error::IllegalParameter("cipher suite"),
            ),
            (
                server_hello_with(|hello| hello.compression_method = 1),
                Error::IllegalParameter("compression method"),
            ),
            (
                server_hello_with(|hello| hello.session_id = vec![0; 33]),
                Error::IllegalParameter("session id length"),
            ),
            (
                server_hello_with(|hello| {
                    let extra = Extension::new(ExtensionType::Other(0x1234), Vec::new());
                    hello.extensions.as_mut().unwrap().push(extra);
                }),
                Error::UnsupportedExtension(ExtensionType::Other(0x1234)),
            ),
            (
                server_hello_with(|hello| {
                    let extensions = hello.extensions.as_mut().unwrap();
                    extensions.push(Extension::extended_master_secret());
                }),
                Error::UnsupportedExtension(ExtendedMasterSecret),
            ),
            (
                server_hello_with(|hello| {
                    let extensions = hello.extensions.as_mut().unwrap();
                    extensions.retain(|e| e.extension_type != ExtendedMasterSecret);
                }),
                Error::HandshakeFailure("server does not support extended master secret"),
            ),
            (
                server_hello_with(|hello| {
                    let extensions = hello.extensions.as_mut().unwrap();
                    let info = extensions
                        .iter_mut()
                        .find(|e| e.extension_type == RenegotiationInfo)
                        .unwrap();
                    info.data = vec![1, 0xaa];
                }),
                Error::HandshakeFailure("renegotiation info is not empty"),
            ),
        ];
        for (record, expected) in cases {
            assert_rejected(&[record], &expected);
        }
    }

    #[test]
    fn unchanged_rebuilt_server_hello_is_the_recorded_one() {
        assert_eq!(server_hello_with(|_| {}), server_records()[SERVER_HELLO]);
    }

    #[test]
    fn key_exchange_the_certificates_key_did_not_sign_is_rejected() {
        let (_, server) = started();
        let unsigned = Error::DecryptError("server key exchange signature");
        let flight = |change: &dyn Fn(&mut Vec<Vec<u8>>)| {
            let mut records = server[..=SERVER_KEY_EXCHANGE].to_vec();
            change(&mut records);
            records
        };
        // A changed signature, a changed key, and a changed server random each break it.
        let last = server[SERVER_KEY_EXCHANGE].len() - 1;
        assert_rejected(&flight(&|r| r[SERVER_KEY_EXCHANGE][last] ^= 1), &unsigned);
        assert_rejected(&flight(&|r| r[SERVER_KEY_EXCHANGE][20] ^= 1), &unsigned);
        assert_rejected(&flight(&|r| r[SERVER_HELLO][20] ^= 1), &unsigned);
        // Another server's certificate with this server's key exchange: the chain is not
        // verified here, but its key still has to be the one that signed.
        let other = plain_record(ContentType::Handshake, &hex(support::CERTIFICATE));
        assert_rejected(&flight(&|r| r[CERTIFICATE] = other.clone()), &unsigned);
    }

    #[test]
    fn key_exchange_on_another_group_or_scheme_is_rejected() {
        let (_, server) = started();
        let rebuilt = |change: &dyn Fn(&mut ServerKeyExchange)| {
            let mut exchange =
                ServerKeyExchange::decode_exact(&server[SERVER_KEY_EXCHANGE][9..]).unwrap();
            change(&mut exchange);
            let body = exchange.to_bytes().unwrap();
            let message = HandshakeMessage::new(HandshakeType::ServerKeyExchange, &body);
            let mut records = server[..SERVER_KEY_EXCHANGE].to_vec();
            records.push(plain_record(ContentType::Handshake, message.bytes()));
            records
        };
        assert_rejected(
            &rebuilt(&|exchange| exchange.params.named_group = 23.into()),
            &Error::IllegalParameter("key exchange group"),
        );
        assert_rejected(
            &rebuilt(&|exchange| exchange.params.curve_type = 1.into()),
            &Error::IllegalParameter("key exchange group"),
        );
        assert_rejected(
            &rebuilt(&|exchange| exchange.params.public.push(0)),
            &Error::IllegalParameter("key exchange public key length"),
        );
        assert_rejected(
            &rebuilt(&|exchange| exchange.algorithm = 0x0401.into()),
            &Error::IllegalParameter("signature algorithm"),
        );
    }

    /// The recording's first flight through the key exchange, with its certificate replaced
    /// by one holding `der`.
    fn flight_with_certificate(der: Vec<u8>) -> Vec<Vec<u8>> {
        let body = Certificate {
            certificate_list: vec![Asn1Cert { der }],
        };
        let message = HandshakeMessage::new(HandshakeType::Certificate, &body.to_bytes().unwrap());
        let mut flight = server_records()[..=SERVER_KEY_EXCHANGE].to_vec();
        flight[CERTIFICATE] = plain_record(ContentType::Handshake, message.bytes());
        flight
    }

    /// The server picks its key before it has proved anything, and verifying with an RSA
    /// key costs time that grows with the cube of its size, so an oversized one is refused
    /// without being used.
    #[test]
    fn certificate_with_an_oversized_rsa_key_is_rejected_without_verifying() {
        let flight = flight_with_certificate(hex(support::OVERSIZED_RSA_CERTIFICATE));
        assert_rejected(
            &flight,
            &Error::HandshakeFailure("server RSA key is not acceptable"),
        );
    }

    // --- The protected part: the test plays the server with the session's own keys ---

    /// A client that has completed its side of the handshake, and a server write state under
    /// the session's keys, with the transcript as it stands before the server's `Finished`.
    struct Session {
        client: Client<'static>,
        server: WriteState,
        master_secret: MasterSecret,
        transcript: Transcript,
        change_cipher_spec: Vec<u8>,
        /// The client and server randoms.
        randoms: ([u8; 32], [u8; 32]),
        /// The records of the client's second flight.
        flight: Vec<Vec<u8>>,
    }

    impl Session {
        /// Runs the recording up to the client's `Finished`.
        fn new() -> Self {
            Self::after(&server_records()[..=SERVER_HELLO_DONE])
        }

        /// Feeds `first_flight` to a fresh client, then re-derives the session's keys from
        /// the client's known private key, the server's recorded public key, and the
        /// handshake messages both sides actually sent.
        fn after(first_flight: &[Vec<u8>]) -> Self {
            let (mut client, _) = started();
            feed(&mut client, first_flight).unwrap();
            let flight = records(&drain(&mut client));

            let client_hello = client_records().remove(0);
            let ReplaySession {
                master_secret,
                transcript,
                randoms,
            } = ReplaySession::derive(&client_hello, first_flight, &flight);
            let keys = master_secret
                .aes128_gcm_keys(&randoms.0, &randoms.1)
                .unwrap();
            let mut write = WriteState::plain();
            let change_cipher_spec = write
                .seal(ContentType::ChangeCipherSpec, VERSION, &[1])
                .unwrap();
            write.protect(keys.server_write);
            Self {
                client,
                server: write,
                master_secret,
                transcript,
                change_cipher_spec,
                randoms,
                flight,
            }
        }

        /// A read state under the keys of whichever side `pick` selects.
        fn reader(&self, pick: fn(TrafficKeys) -> Aes128GcmProtection) -> ReadState {
            let keys = self
                .master_secret
                .aes128_gcm_keys(&self.randoms.0, &self.randoms.1)
                .unwrap();
            let mut state = ReadState::plain();
            state.protect(pick(keys));
            state
        }

        /// A record from the server under the session's keys.
        fn seal(&mut self, content_type: ContentType, fragment: &[u8]) -> Vec<u8> {
            self.server.seal(content_type, VERSION, fragment).unwrap()
        }

        /// The server's `Finished` message with the right check value.
        fn finished(&self) -> Vec<u8> {
            let verify = self
                .master_secret
                .finished_verify_data(ConnectionEnd::Server, &self.transcript.hash())
                .unwrap();
            HandshakeMessage::new(HandshakeType::Finished, &verify)
                .bytes()
                .to_vec()
        }

        /// Sends the server's `ChangeCipherSpec` and `Finished`, completing the handshake.
        fn establish(mut self) -> Self {
            let finished = self.finished();
            let finished = self.seal(ContentType::Handshake, &finished);
            let flight = [self.change_cipher_spec.clone(), finished];
            feed(&mut self.client, &flight).unwrap();
            assert!(self.client.is_established());
            self
        }

        /// The recording's handshake, completed.
        fn established() -> Self {
            Self::new().establish()
        }
    }

    /// Ties the harness to OpenSSL: under the keys the harness derived, the `Finished`
    /// record OpenSSL sent opens to the very message the harness would send.
    #[test]
    fn finished_the_test_server_builds_is_the_one_openssl_sent() {
        let session = Session::new();
        let mut reader = session.reader(|keys| keys.server_write);
        let recorded = &server_records()[SERVER_FINISHED];
        assert_eq!(reader.receive(recorded), recorded.len());
        let opened = reader.next_record().unwrap().unwrap();
        assert_eq!(opened.fragment, session.finished());
    }

    #[test]
    fn finished_of_the_wrong_length_is_rejected() {
        let mut session = Session::new();
        let mut finished = session.finished();
        finished.pop();
        finished[3] -= 1; // the message's length byte
        let record = session.seal(ContentType::Handshake, &finished);
        let flight = [session.change_cipher_spec.clone(), record];
        assert_eq!(
            feed(&mut session.client, &flight),
            Err(Error::IllegalParameter("Finished length"))
        );
        assert!(!session.client.is_established());
    }

    #[test]
    fn change_cipher_spec_must_be_the_single_byte_one_and_follow_whole_messages() {
        let mut session = Session::new();
        let wrong = plain_record(ContentType::ChangeCipherSpec, &[2]);
        assert_eq!(
            feed(&mut session.client, &[wrong]),
            Err(Error::IllegalParameter("ChangeCipherSpec body"))
        );

        let mut session = Session::new();
        let half_a_header = plain_record(ContentType::Handshake, &[20, 0]);
        let flight = [half_a_header, session.change_cipher_spec.clone()];
        assert_eq!(
            feed(&mut session.client, &flight),
            Err(Error::UnexpectedMessage(
                "record interleaved with a handshake message"
            ))
        );
        assert!(!session.client.is_established());
    }

    // --- A certificate request ---

    /// The recording's first flight with `request` inserted before the `ServerHelloDone`.
    fn flight_requesting_a_certificate(request: &[u8]) -> Vec<Vec<u8>> {
        let message = HandshakeMessage::new(HandshakeType::CertificateRequest, request);
        let mut flight = server_records()[..=SERVER_HELLO_DONE].to_vec();
        let record = plain_record(ContentType::Handshake, message.bytes());
        flight.insert(SERVER_HELLO_DONE, record);
        flight
    }

    /// RSA-sign certificates, RSA-PSS with SHA-256, no authorities named.
    const CERTIFICATE_REQUEST: [u8; 8] = [1, 1, 0, 2, 0x08, 0x04, 0, 0];

    #[test]
    fn certificate_request_is_answered_with_an_empty_certificate_in_the_transcript() {
        let session = Session::after(&flight_requesting_a_certificate(&CERTIFICATE_REQUEST));
        // An empty certificate list, then the key exchange.
        assert_eq!(session.flight[0][5..], [11, 0, 0, 3, 0, 0, 0]);
        assert_eq!(session.flight[1][5], 16);

        // The harness hashed the request and that reply. If the client had not, the two
        // would hold different keys and this would fail.
        let mut reader = session.reader(|keys| keys.client_write);
        let finished = &session.flight[3];
        assert_eq!(reader.receive(finished), finished.len());
        assert_eq!(reader.next_record().unwrap().unwrap().fragment[0], 20);
        assert!(session.establish().client.is_established());
    }

    #[test]
    fn second_or_malformed_certificate_request_is_rejected() {
        let mut twice = flight_requesting_a_certificate(&CERTIFICATE_REQUEST);
        twice.insert(SERVER_HELLO_DONE, twice[SERVER_HELLO_DONE].clone());
        assert_rejected(
            &twice,
            &Error::UnexpectedMessage("handshake message out of order"),
        );

        let (mut client, _) = started();
        let malformed = flight_requesting_a_certificate(&[0xff, 0xff]);
        let error = feed(&mut client, &malformed).unwrap_err();
        assert!(
            matches!(
                error,
                Error::Codec {
                    what: "certificate request",
                    ..
                }
            ),
            "{error:?}"
        );
        assert_alert_queued(&mut client, Some(AlertDescription::DecodeError));
    }

    // --- The trust decision and the certificate's key ---

    #[test]
    fn chain_that_fails_path_validation_ends_the_handshake() {
        let config = ClientConfig {
            server_name: Some("tls.test"),
            verification: ServerVerification::Path {
                trust_anchors: &[],
                time: Time::new(2026, 11, 17, 0, 0, 0).unwrap(),
                dns_name: "tls.test",
            },
        };
        let mut client = Client::new(config, &mut CountingRandom::default()).unwrap();
        drain(&mut client);
        let server = server_records();
        let error = feed(&mut client, &server[..=CERTIFICATE]).unwrap_err();
        assert!(
            matches!(error, Error::Certificate(CertificateError::Path(_))),
            "{error:?}"
        );
        assert_alert_queued(&mut client, Some(AlertDescription::CertificateUnknown));
        assert!(
            client.peer_chain().is_none(),
            "a refused chain is not reported"
        );
    }

    #[test]
    fn certificate_whose_key_is_not_rsa_is_rejected() {
        let key = Ed25519SigningKey::from_seed([9; 32]);
        let signer = Ed25519CertificateSigner::new(&key);
        let name = NameDer::common_name("tls.test").unwrap();
        let der = CertificateBuilder::certificate_authority(&[1], name.clone(), name, None)
            .unwrap()
            .validity(
                Time::new(2026, 1, 1, 0, 0, 0).unwrap(),
                Time::new(2027, 1, 1, 0, 0, 0).unwrap(),
            )
            .unwrap()
            .subject_public_key_info(signer.subject_public_key_info().unwrap())
            .sign(&signer)
            .unwrap();
        let flight = flight_with_certificate(der.as_bytes().to_vec());
        assert_rejected(
            &flight,
            &Error::HandshakeFailure("server certificate key is not RSA"),
        );
    }

    #[test]
    fn finished_with_the_wrong_check_value_is_rejected() {
        let mut session = Session::new();
        let mut finished = session.finished();
        *finished.last_mut().unwrap() ^= 1;
        let record = session.seal(ContentType::Handshake, &finished);
        let flight = [session.change_cipher_spec.clone(), record];
        let error = feed(&mut session.client, &flight).unwrap_err();
        assert_eq!(error, Error::DecryptError("server Finished"));
        assert!(!session.client.is_established());
        assert!(
            !drain(&mut session.client).is_empty(),
            "a sealed alert is queued"
        );
    }

    #[test]
    fn finished_without_a_change_cipher_spec_is_rejected() {
        // Under the new keys but with no ChangeCipherSpec, the record is noise to a reader
        // still expecting plaintext.
        let mut session = Session::new();
        let finished = session.finished();
        let record = session.seal(ContentType::Handshake, &finished);
        assert!(feed(&mut session.client, &[record]).is_err());
        assert!(!session.client.is_established());

        // In the clear, it is a handshake message where a ChangeCipherSpec must come first.
        let mut session = Session::new();
        let record = plain_record(ContentType::Handshake, &session.finished());
        let error = feed(&mut session.client, &[record]).unwrap_err();
        assert_eq!(
            error,
            Error::UnexpectedMessage("handshake message out of order")
        );
        assert!(!session.client.is_established());
    }

    #[test]
    fn application_data_before_the_servers_finished_is_rejected() {
        let mut session = Session::new();
        let data = session.seal(ContentType::ApplicationData, b"too soon");
        let flight = [session.change_cipher_spec.clone(), data];
        let error = feed(&mut session.client, &flight).unwrap_err();
        assert_eq!(
            error,
            Error::UnexpectedMessage("application data during the handshake")
        );
        assert_eq!(session.client.plaintext(), []);
    }

    #[test]
    fn altered_protected_record_ends_the_connection_but_keeps_earlier_plaintext() {
        let mut session = Session::established();
        let good = session.seal(ContentType::ApplicationData, b"kept");
        let mut bad = session.seal(ContentType::ApplicationData, b"lost");
        *bad.last_mut().unwrap() ^= 1;
        let error = feed(&mut session.client, &[good, bad]).unwrap_err();
        assert_eq!(error, Error::Record(RecordError::BadRecordMac));
        assert_eq!(session.client.plaintext(), b"kept");
        assert_eq!(session.client.write(b"x"), Err(Error::InvalidState));
        assert_eq!(session.client.close(), Err(Error::InvalidState));
        assert_eq!(session.client.receive_eof(), Err(Error::InvalidState));
    }

    // --- An established connection ---

    #[test]
    fn write_is_refused_until_the_handshake_finishes_without_ending_it() {
        let (mut client, server) = started();
        assert_eq!(client.write(b"early"), Err(Error::InvalidState));
        feed(&mut client, &server[..=SERVER_FINISHED]).unwrap();
        assert!(
            client.is_established(),
            "the refusal did not fail the connection"
        );
    }

    #[test]
    fn end_of_input_without_close_notify_is_truncation() {
        let mut session = Session::established();
        assert_eq!(session.client.receive_eof(), Err(Error::Truncated));
        assert_eq!(session.client.receive(&[0]), Err(Error::InvalidState));
    }

    #[test]
    fn peers_close_notify_is_answered_and_later_bytes_are_discarded() {
        let mut session = Session::established();
        drain(&mut session.client);
        let close = session.seal(ContentType::Alert, &[1, 0]);
        let late = session.seal(ContentType::ApplicationData, b"after close");
        feed(&mut session.client, &[close, late]).unwrap();
        assert!(session.client.peer_closed());
        assert_eq!(session.client.plaintext(), []);
        assert!(!session.client.wants_receive());
        assert!(!drain(&mut session.client).is_empty(), "our close_notify");
        assert_eq!(session.client.write(b"x"), Err(Error::InvalidState));
        assert_eq!(session.client.receive_eof(), Ok(()));
    }

    #[test]
    fn close_stops_writes_but_not_reads() {
        let mut session = Session::established();
        session.client.close().unwrap();
        session.client.close().unwrap();
        let alerts = records(&drain(&mut session.client));
        assert_eq!(alerts.len(), 1, "close_notify is sent once");
        assert_eq!(session.client.write(b"x"), Err(Error::InvalidState));
        let data = session.seal(ContentType::ApplicationData, b"still coming");
        feed(&mut session.client, &[data]).unwrap();
        assert_eq!(session.client.plaintext(), b"still coming");
    }

    #[test]
    fn write_splits_data_into_records_and_pauses_at_the_transmit_limit() {
        let mut session = Session::established();
        drain(&mut session.client);
        let data = vec![0x5a; 200_000];
        let accepted = session.client.write(&data).unwrap();
        // Each record carries 2^14 bytes and adds a 5-byte header, 8-byte nonce and tag.
        let record = 16_384 + 5 + 24;
        let count = Client::TRANSMIT_LIMIT.div_ceil(record);
        assert_eq!(accepted, count * 16_384);
        assert_eq!(session.client.transmit().len(), count * record);
        assert_eq!(
            session.client.write(&data),
            Ok(0),
            "full until some is sent"
        );
        session.client.sent(record);
        assert_eq!(session.client.write(&data), Ok(16_384));
    }

    #[test]
    fn receive_pauses_while_plaintext_is_not_consumed() {
        let mut session = Session::established();
        let chunk = vec![0xa5; 16_384];
        // Six full records: three times what the client will hold at once.
        let stream: Vec<u8> = (0..6)
            .flat_map(|_| session.seal(ContentType::ApplicationData, &chunk))
            .collect();
        let mut offset = 0;
        let mut total = 0;
        while offset < stream.len() {
            let taken = session.client.receive(&stream[offset..]).unwrap();
            offset += taken;
            let held = session.client.plaintext().len();
            assert!(held <= 2 * Client::PLAINTEXT_LIMIT, "{held} bytes held");
            if taken == 0 {
                assert!(!session.client.wants_receive());
            }
            total += held;
            session.client.consume(held);
        }
        assert_eq!(total, 6 * 16_384);
    }

    #[test]
    fn hello_request_is_refused_with_a_warning_a_bounded_number_of_times() {
        let mut session = Session::established();
        drain(&mut session.client);
        let request = [0, 0, 0, 0];
        for _ in 0..4 {
            let record = session.seal(ContentType::Handshake, &request);
            feed(&mut session.client, &[record]).unwrap();
            assert_eq!(records(&drain(&mut session.client)).len(), 1, "one warning");
        }
        let record = session.seal(ContentType::Handshake, &request);
        assert_eq!(
            feed(&mut session.client, &[record]),
            Err(Error::UnexpectedMessage("too many hello requests"))
        );
    }

    #[test]
    fn other_handshake_messages_after_the_handshake_are_rejected() {
        let mut session = Session::established();
        let finished = session.finished();
        let record = session.seal(ContentType::Handshake, &finished);
        assert_eq!(
            feed(&mut session.client, &[record]),
            Err(Error::UnexpectedMessage("handshake message out of order"))
        );
    }

    #[test]
    fn flood_of_empty_records_is_cut_off() {
        let mut session = Session::established();
        for _ in 0..16 {
            let record = session.seal(ContentType::ApplicationData, &[]);
            feed(&mut session.client, &[record]).unwrap();
        }
        let record = session.seal(ContentType::ApplicationData, &[]);
        assert_eq!(
            feed(&mut session.client, &[record]),
            Err(Error::UnexpectedMessage("too many empty records"))
        );
    }

    #[test]
    fn alert_other_than_close_notify_ends_an_established_connection() {
        let mut session = Session::established();
        let alert = Alert::new(AlertLevel::Warning, AlertDescription::UserCanceled);
        let record = session.seal(ContentType::Alert, &alert.to_bytes().unwrap());
        assert_eq!(
            feed(&mut session.client, &[record]),
            Err(Error::PeerAlert(alert))
        );
        assert_eq!(session.client.receive(&[0]), Err(Error::InvalidState));
    }

    #[test]
    fn hello_request_with_a_body_is_rejected() {
        let mut session = Session::established();
        let record = session.seal(ContentType::Handshake, &[0, 0, 0, 1, 0xaa]);
        assert_eq!(
            feed(&mut session.client, &[record]),
            Err(Error::IllegalParameter("HelloRequest body"))
        );
    }

    /// Records the client had to leave with the caller while its plaintext backlog was full
    /// are processed when offered again, a closing alert included.
    #[test]
    fn records_behind_a_full_backlog_are_not_lost() {
        let mut session = Session::established();
        drain(&mut session.client);
        let stream = [
            session.seal(ContentType::ApplicationData, &[0x11; 16_384]),
            session.seal(ContentType::ApplicationData, b"tail"),
            session.seal(ContentType::Alert, &[1, 0]),
        ]
        .concat();
        let taken = session.client.receive(&stream).unwrap();
        assert!(taken < stream.len(), "paused at the backlog");
        assert!(!session.client.wants_receive());
        assert_eq!(session.client.plaintext().len(), 16_384);
        session.client.consume(16_384);

        assert!(session.client.wants_receive());
        let rest = &stream[taken..];
        assert_eq!(session.client.receive(rest), Ok(rest.len()));
        assert_eq!(session.client.plaintext(), b"tail");
        assert!(session.client.peer_closed());
        assert_eq!(session.client.receive_eof(), Ok(()));
    }

    #[test]
    fn closing_before_the_handshake_finishes_abandons_the_connection() {
        let (mut client, server) = started();
        feed(&mut client, &server[..=SERVER_HELLO]).unwrap();
        client.close().unwrap();
        let queued = records(&drain(&mut client));
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0][5..], [1, 0], "close_notify, in the clear");
        assert_eq!(
            client.receive(&server[CERTIFICATE]),
            Err(Error::InvalidState)
        );
        assert_eq!(drain(&mut client), [], "nothing follows a close_notify");
        assert!(!client.is_established());
    }

    #[test]
    fn random_source_that_fails_is_reported_before_anything_is_sent() {
        struct Empty;
        impl crypto::RandomSource for Empty {
            fn fill_bytes(&mut self, _: &mut [u8]) -> crypto::Result<()> {
                Err(crypto::CryptoError::EntropyUnavailable)
            }
        }
        let config = ClientConfig {
            server_name: None,
            verification: ServerVerification::Unverified,
        };
        let error = Client::new(config, &mut Empty).err();
        assert_eq!(
            error,
            Some(Error::Crypto(crypto::CryptoError::EntropyUnavailable))
        );
    }
}
