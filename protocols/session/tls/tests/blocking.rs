//! The blocking stream over a scripted transport that plays the recorded OpenSSL connection
//! (`support`): the server's bytes become readable only once the client has written what
//! came before them, as on a real connection.

mod support;

mod blocking {
    use std::io::{self, Read, Write};

    use tls::{
        ClientConfig, Error as TlsError, ServerVerification,
        blocking::{Error, Stream},
        record::WriteState,
        wire::{ContentType, ProtocolVersion},
    };

    use super::support::{self, CountingRandom, ReplaySession, hex};

    const REQUEST: &[u8] = b"hello tls\n";
    const RESPONSE: &[u8] = b"slt olleh\n";

    /// A transport that replays one side of a conversation. Each step is how many bytes the
    /// client must have put on the wire in total before the step's bytes can be read. A read
    /// that comes before those bytes are on the wire is a deadlock on a real socket, so here
    /// it panics. Once every step has been read, a read returns `end`.
    struct Script {
        steps: Vec<(usize, Vec<u8>)>,
        /// What has reached the wire.
        written: Vec<u8>,
        /// What has been written but, when `buffers` is set, not flushed yet.
        unflushed: Vec<u8>,
        /// The most bytes one read returns.
        chunk: usize,
        /// The most bytes one write accepts.
        write_chunk: usize,
        /// Whether written bytes reach the wire only on `flush`, like a `BufWriter`.
        buffers: bool,
        /// What a read returns once the script is exhausted.
        end: fn() -> io::Result<usize>,
    }

    impl Read for Script {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let Some((needed, bytes)) = self.steps.first_mut() else {
                return (self.end)();
            };
            assert!(
                self.written.len() >= *needed,
                "the client read before writing: {} of {needed} bytes written",
                self.written.len()
            );
            let count = bytes.len().min(buffer.len()).min(self.chunk);
            buffer[..count].copy_from_slice(&bytes[..count]);
            bytes.drain(..count);
            if bytes.is_empty() {
                self.steps.remove(0);
            }
            Ok(count)
        }
    }

    impl Write for Script {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            let count = buffer.len().min(self.write_chunk);
            if self.buffers {
                self.unflushed.extend(&buffer[..count]);
            } else {
                self.written.extend(&buffer[..count]);
            }
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.written.append(&mut self.unflushed);
            Ok(())
        }
    }

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

    /// The recording as a script: the server's four flights, each released by the client
    /// flight that preceded it.
    fn recorded(chunk: usize) -> Script {
        let client = records(&hex(support::REPLAY_CLIENT));
        let server = records(&hex(support::REPLAY_SERVER));
        let sent = |count: usize| client[..count].concat().len();
        Script {
            steps: vec![
                (sent(1), server[..4].concat()),  // after the ClientHello
                (sent(4), server[4..6].concat()), // after our Finished
                (sent(5), server[6].clone()),     // after the request
                (sent(6), server[7].clone()),     // after our close_notify
            ],
            written: Vec::new(),
            unflushed: Vec::new(),
            chunk,
            write_chunk: usize::MAX,
            buffers: false,
            end: || Ok(0),
        }
    }

    fn config() -> ClientConfig<'static> {
        ClientConfig {
            server_name: Some("tls.test"),
            verification: ServerVerification::Unverified,
        }
    }

    fn connect(script: Script) -> Result<Stream<'static, Script>, Error> {
        Stream::connect(script, config(), &mut CountingRandom::default())
    }

    #[test]
    fn stream_carries_the_recorded_exchange_and_sends_the_recorded_bytes() {
        for chunk in [usize::MAX, 1, 100] {
            let mut stream = connect(recorded(chunk)).unwrap();
            assert!(stream.client().is_established());
            assert_eq!(stream.client().session_id().map(<[u8]>::len), Some(32));

            stream.write_all(REQUEST).unwrap();
            let mut response = [0; 10];
            stream.read_exact(&mut response).unwrap();
            assert_eq!(response, RESPONSE);

            stream.close().unwrap();
            assert_eq!(
                stream.read(&mut response).unwrap(),
                0,
                "the peer closed too"
            );
            assert_eq!(stream.read(&mut response).unwrap(), 0, "and stays closed");
            assert_eq!(
                stream.get_ref().written,
                hex(support::REPLAY_CLIENT),
                "{chunk}"
            );
        }
    }

    #[test]
    fn read_returns_only_what_the_buffer_holds_and_keeps_the_rest() {
        let mut stream = connect(recorded(usize::MAX)).unwrap();
        stream.write_all(REQUEST).unwrap();
        let mut small = [0; 4];
        assert_eq!(stream.read(&mut small).unwrap(), 4);
        assert_eq!(small, RESPONSE[..4]);
        let mut rest = Vec::new();
        stream.close().unwrap();
        stream.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, RESPONSE[4..]);
        assert_eq!(stream.read(&mut []).unwrap(), 0);
    }

    #[test]
    fn transport_ending_without_close_notify_is_an_unexpected_eof() {
        let mut script = recorded(usize::MAX);
        script.steps.pop(); // the server never sends its close_notify
        let mut stream = connect(script).unwrap();
        stream.write_all(REQUEST).unwrap();
        let mut response = Vec::new();
        let error = stream.read_to_end(&mut response).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(
            response, RESPONSE,
            "what arrived before the cut is still delivered"
        );
    }

    #[test]
    fn transport_ending_during_the_handshake_fails_the_connect() {
        let mut script = recorded(usize::MAX);
        script.steps.truncate(1);
        let error = connect(script).err().unwrap();
        assert!(
            matches!(error, Error::Tls(TlsError::Truncated)),
            "{error:?}"
        );
    }

    #[test]
    fn handshake_failure_is_reported_after_the_alert_has_been_sent() {
        let mut script = recorded(usize::MAX);
        // The flight ends with the 9-byte ServerHelloDone record; the byte before it is the
        // last byte of the key-exchange signature.
        let signature_end = script.steps[0].1.len() - 9 - 1;
        script.steps[0].1[signature_end] ^= 1;
        script.steps.truncate(1);
        let hello = records(&hex(support::REPLAY_CLIENT))[0].len();

        // `&mut Script` is a transport too, which leaves the script here to inspect.
        let result = Stream::connect(&mut script, config(), &mut CountingRandom::default());
        let error = result.err().unwrap();
        assert!(
            matches!(error, Error::Tls(TlsError::DecryptError(_))),
            "{error:?}"
        );
        // A fatal decrypt_error alert, in the clear, right after the ClientHello.
        assert_eq!(script.written[hello..], [21, 3, 3, 0, 2, 2, 51]);
    }

    #[test]
    fn transport_error_is_passed_through_unchanged() {
        let mut script = recorded(usize::MAX);
        script.steps.pop();
        script.end = || Err(io::Error::from(io::ErrorKind::ConnectionReset));
        let mut stream = connect(script).unwrap();
        stream.write_all(REQUEST).unwrap();
        let mut response = Vec::new();
        let error = stream.read_to_end(&mut response).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
    }

    #[test]
    fn write_after_close_is_refused_as_not_connected() {
        let mut stream = connect(recorded(usize::MAX)).unwrap();
        stream.write_all(REQUEST).unwrap();
        stream.close().unwrap();
        let error = stream.write(b"more").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotConnected);
    }

    /// Fails every other call with `Interrupted`.
    struct Flaky(Script, bool);
    impl Flaky {
        fn interrupt(&mut self) -> io::Result<()> {
            self.1 = !self.1;
            if self.1 {
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            Ok(())
        }
    }
    impl Read for Flaky {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.interrupt()?;
            self.0.read(buffer)
        }
    }
    impl Write for Flaky {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.interrupt()?;
            self.0.write(buffer)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn interrupted_reads_and_writes_are_retried() {
        let transport = Flaky(recorded(usize::MAX), false);
        let mut stream =
            Stream::connect(transport, config(), &mut CountingRandom::default()).unwrap();
        stream.write_all(REQUEST).unwrap();
        let mut response = [0; 10];
        stream.read_exact(&mut response).unwrap();
        assert_eq!(response, RESPONSE);
    }

    #[test]
    fn transport_that_accepts_a_few_bytes_per_write_gets_every_byte_once() {
        let mut script = recorded(usize::MAX);
        script.write_chunk = 7;
        let mut stream = connect(script).unwrap();
        stream.write_all(REQUEST).unwrap();
        stream.close().unwrap();
        assert_eq!(stream.get_ref().written, hex(support::REPLAY_CLIENT));
    }

    #[test]
    fn transport_that_buffers_writes_is_flushed_before_waiting_for_the_peer() {
        let mut script = recorded(usize::MAX);
        script.buffers = true;
        let mut stream = connect(script).unwrap();
        stream.write_all(REQUEST).unwrap();
        let mut response = [0; 10];
        stream.read_exact(&mut response).unwrap();
        assert_eq!(response, RESPONSE);
        stream.close().unwrap();
        assert_eq!(stream.get_ref().written, hex(support::REPLAY_CLIENT));
    }

    #[test]
    fn transport_that_accepts_nothing_is_a_write_zero_error() {
        let mut script = recorded(usize::MAX);
        script.write_chunk = 0;
        let error = connect(script).err().unwrap();
        assert!(
            matches!(&error, Error::Io(io) if io.kind() == io::ErrorKind::WriteZero),
            "{error:?}"
        );
    }

    /// A server write state under the recorded session's keys, positioned after the
    /// `Finished` the recording's server sent.
    fn server_writer() -> WriteState {
        let client = records(&hex(support::REPLAY_CLIENT));
        let server = records(&hex(support::REPLAY_SERVER));
        let session = ReplaySession::derive(&client[0], &server[..4], &client[1..4]);
        let mut writer = WriteState::plain();
        writer.protect(session.keys().server_write);
        // The recorded Finished used sequence number 0.
        writer
            .seal(ContentType::Handshake, ProtocolVersion::TLS_1_2, &[0])
            .unwrap();
        writer
    }

    /// The recording up to the server's `Finished`, then `records` from the test, all
    /// readable as soon as the client's handshake flight is on the wire.
    fn recorded_then(records_after: &[Vec<u8>]) -> Script {
        let mut script = recorded(usize::MAX);
        script.steps.truncate(2);
        script.steps[1].1.extend(records_after.concat());
        script
    }

    /// More plaintext than the client holds at once arrives in a single read, with the
    /// server's `close_notify` behind it. Everything is delivered in order, nothing is read
    /// from the transport while records are still pending, and the close is answered.
    #[test]
    fn records_pending_behind_a_plaintext_backlog_are_all_delivered() {
        let mut writer = server_writer();
        let version = ProtocolVersion::TLS_1_2;
        let mut flight = Vec::new();
        let mut expected = Vec::new();
        for fill in [0x11, 0x22, 0x33] {
            let data = vec![fill; 16_384];
            flight.push(
                writer
                    .seal(ContentType::ApplicationData, version, &data)
                    .unwrap(),
            );
            expected.extend(data);
        }
        flight.push(writer.seal(ContentType::Alert, version, &[1, 0]).unwrap());

        let mut script = recorded_then(&flight);
        // After the last byte the peer is gone: reading again would be a bug.
        script.end = || panic!("read from the transport after the peer closed");
        let mut stream = connect(script).unwrap();
        let mut received = Vec::new();
        stream.read_to_end(&mut received).unwrap();
        assert_eq!(received.len(), expected.len());
        assert!(
            received == expected,
            "the plaintext arrived intact and in order"
        );

        let sent = records(&stream.get_ref().written);
        assert_eq!(sent.len(), 5, "the handshake's four records and one reply");
        assert_eq!(sent[4][0], 21, "the reply to the server's close_notify");
    }

    #[test]
    fn read_after_a_failed_read_fails_again_instead_of_waiting() {
        let mut writer = server_writer();
        let version = ProtocolVersion::TLS_1_2;
        let mut record = writer
            .seal(ContentType::ApplicationData, version, b"damaged")
            .unwrap();
        *record.last_mut().unwrap() ^= 1;
        // In place of the recorded response: readable once the request is on the wire.
        let mut script = recorded(usize::MAX);
        script.steps.truncate(3);
        script.steps[2].1 = record;
        let mut stream = connect(script).unwrap();
        stream.write_all(REQUEST).unwrap();

        let mut buffer = [0; 16];
        let first = stream.read(&mut buffer).unwrap_err();
        assert_eq!(first.kind(), io::ErrorKind::InvalidData);
        let sent = records(&stream.get_ref().written);
        assert_eq!(sent.last().unwrap()[0], 21, "the fatal alert was sent");

        let second = stream.read(&mut buffer).unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::NotConnected);
        let write = stream.write(b"x").unwrap_err();
        assert_eq!(write.kind(), io::ErrorKind::NotConnected);
    }

    /// Fails the write that would cross `fail_at` bytes, once.
    struct FailsOnce {
        script: Script,
        fail_at: usize,
        failed: bool,
    }
    impl Read for FailsOnce {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.script.read(buffer)
        }
    }
    impl Write for FailsOnce {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if !self.failed && self.script.written.len() >= self.fail_at {
                self.failed = true;
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
            self.script.write(buffer)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.script.flush()
        }
    }

    /// A transport error partway through sending a record loses nothing and repeats nothing:
    /// the write still counts, and the next call sends the rest.
    #[test]
    fn write_interrupted_by_a_transport_error_resumes_without_repeating_bytes() {
        let client = records(&hex(support::REPLAY_CLIENT));
        let mut script = recorded(usize::MAX);
        script.write_chunk = 7;
        let transport = FailsOnce {
            script,
            fail_at: client[..4].concat().len() + 14, // partway through the request record
            failed: false,
        };
        let mut stream =
            Stream::connect(transport, config(), &mut CountingRandom::default()).unwrap();

        // The request was accepted and queued, though the transport timed out partway.
        assert_eq!(stream.write(REQUEST).unwrap(), REQUEST.len());
        let on_the_wire = stream.get_ref().script.written.len();
        assert!(
            on_the_wire < client[..5].concat().len(),
            "the record was cut short"
        );
        stream.flush().unwrap();

        let mut response = [0; 10];
        stream.read_exact(&mut response).unwrap();
        assert_eq!(response, RESPONSE);
        stream.close().unwrap();
        assert_eq!(stream.get_ref().script.written, hex(support::REPLAY_CLIENT));
    }

    /// More data than the stream queues at once, with the transport failing once at the
    /// start: the first `write` accepts a full queue it could not send, and the next must
    /// send that before it can accept more, or `write_all` would see a write of zero bytes.
    #[test]
    fn large_write_resumes_after_a_transport_error_instead_of_stalling() {
        let client = records(&hex(support::REPLAY_CLIENT));
        let handshake = client[..4].concat().len();
        let transport = FailsOnce {
            script: recorded(usize::MAX),
            fail_at: handshake,
            failed: false,
        };
        let mut stream =
            Stream::connect(transport, config(), &mut CountingRandom::default()).unwrap();

        // Five full records: one more than the queue holds.
        let data = vec![0x7e; 5 * 16_384];
        stream.write_all(&data).unwrap();
        stream.flush().unwrap();
        let sent = records(&stream.get_ref().script.written[handshake..]);
        assert_eq!(sent.len(), 5);
        assert!(
            sent.iter()
                .all(|record| record.len() == 5 + 8 + 16_384 + 16)
        );
    }
}
