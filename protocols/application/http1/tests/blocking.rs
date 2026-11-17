//! The blocking exchange over a scripted transport: what it writes, how much it reads, and
//! what it makes of the ways a connection can end.

mod blocking {
    use std::{
        collections::VecDeque,
        io::{self, Read, Write},
    };

    use http1::{
        Error as HttpError, Limits, Request,
        blocking::{Error, exchange},
    };

    const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    const UNTIL_CLOSE: &[u8] = b"HTTP/1.1 200 OK\r\n\r\nall of it";

    /// A transport that plays a list of read results in order and records what is written.
    /// Reading before the request has been flushed, or after the list is exhausted, is a
    /// deadlock on a real connection, so here it panics.
    #[derive(Default)]
    struct Script {
        reads: VecDeque<io::Result<Vec<u8>>>,
        unflushed: Vec<u8>,
        written: Vec<u8>,
    }

    impl Script {
        /// A script whose reads return `pieces`, then whatever `end` is.
        fn of(pieces: &[&[u8]], end: Option<io::Result<Vec<u8>>>) -> Self {
            let reads = pieces.iter().map(|piece| Ok(piece.to_vec()));
            Self {
                reads: reads.chain(end).collect(),
                ..Self::default()
            }
        }
    }

    impl Read for Script {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            assert!(
                self.unflushed.is_empty(),
                "read before the request was flushed"
            );
            let mut bytes = self
                .reads
                .pop_front()
                .expect("read past the script's end")?;
            let count = bytes.len().min(buffer.len());
            buffer[..count].copy_from_slice(&bytes[..count]);
            if count < bytes.len() {
                self.reads.push_front(Ok(bytes.split_off(count)));
            }
            Ok(count)
        }
    }

    impl Write for Script {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            // A few bytes at a time, as a slow connection would take them.
            let count = buffer.len().min(7);
            self.unflushed.extend(&buffer[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.written.append(&mut self.unflushed);
            Ok(())
        }
    }

    fn request() -> Request {
        Request::get("example.test", "/").unwrap()
    }

    /// How a scripted connection ends, if the script says.
    type End = Option<io::Result<Vec<u8>>>;

    /// A clean end of input.
    const EOF: End = Some(Ok(Vec::new()));

    /// A read that fails with this kind of error.
    fn fails(kind: io::ErrorKind) -> io::Result<Vec<u8>> {
        Err(io::Error::from(kind))
    }

    #[test]
    fn request_is_sent_whole_and_the_response_read_to_its_end_and_no_further() {
        // No end is scripted: a read after the response is complete would panic.
        let pieces: Vec<&[u8]> = RESPONSE.chunks(3).collect();
        let mut transport = Script::of(&pieces, None);
        let response = exchange(&mut transport, &request(), Limits::default()).unwrap();
        assert_eq!(transport.written, request().encode());
        assert_eq!(response.head.status, 200);
        assert_eq!(response.body, b"hello");
        assert!(response.keep_alive);
    }

    #[test]
    fn asking_to_close_or_surplus_bytes_make_the_connection_unusable() {
        let closing = request().header("connection", "keep-alive, Close").unwrap();
        let mut transport = Script::of(&[RESPONSE], None);
        let response = exchange(&mut transport, &closing, Limits::default()).unwrap();
        assert!(
            !response.keep_alive,
            "the request asked for the connection to close"
        );

        let with_surplus = [RESPONSE, b"HTTP/1.1 200 OK\r\n"].concat();
        let mut transport = Script::of(&[&with_surplus], None);
        let response = exchange(&mut transport, &request(), Limits::default()).unwrap();
        assert_eq!(response.body, b"hello", "the response itself is intact");
        assert!(
            !response.keep_alive,
            "the server sent bytes nothing asked for"
        );

        let mut old = request();
        old.version = b"HTTP/1.0".to_vec();
        let mut transport = Script::of(&[RESPONSE], None);
        let response = exchange(&mut transport, &old, Limits::default()).unwrap();
        assert!(!response.keep_alive, "an HTTP/1.0 request does not keep it");
    }

    #[test]
    fn body_that_runs_to_the_end_needs_a_clean_end_of_input() {
        let mut transport = Script::of(&[UNTIL_CLOSE], EOF);
        let response = exchange(&mut transport, &request(), Limits::default()).unwrap();
        assert_eq!(response.body, b"all of it");
        assert!(!response.keep_alive);

        // A TLS stream reports a connection that ended without close_notify this way.
        let unconfirmed = || Some(fails(io::ErrorKind::UnexpectedEof));
        let mut transport = Script::of(&[UNTIL_CLOSE], unconfirmed());
        let error = exchange(&mut transport, &request(), Limits::default()).unwrap_err();
        assert!(
            matches!(error, Error::Http(HttpError::UncleanClose)),
            "{error:?}"
        );

        let limits = Limits::default().accepting_unclean_close();
        let mut transport = Script::of(&[UNTIL_CLOSE], unconfirmed());
        let response = exchange(&mut transport, &request(), limits).unwrap();
        assert_eq!(response.body, b"all of it");
    }

    #[test]
    fn connection_that_ends_early_is_an_error_that_says_how_early() {
        let outcome = |pieces: &[&[u8]], end| {
            let mut transport = Script::of(pieces, end);
            match exchange(&mut transport, &request(), Limits::default()) {
                Err(Error::Http(error)) => error,
                other => panic!("expected an HTTP error, got {other:?}"),
            }
        };
        assert_eq!(outcome(&[], EOF), HttpError::ClosedBeforeResponse);
        assert_eq!(outcome(&[&RESPONSE[..20]], EOF), HttpError::Truncated);
        assert_eq!(
            outcome(&[&RESPONSE[..RESPONSE.len() - 1]], EOF),
            HttpError::Truncated
        );
        let unconfirmed = Some(fails(io::ErrorKind::UnexpectedEof));
        assert_eq!(
            outcome(&[&RESPONSE[..20]], unconfirmed),
            HttpError::Truncated
        );
    }

    #[test]
    fn interrupted_read_is_retried_and_other_transport_errors_are_passed_on() {
        let mut transport = Script::of(&[&RESPONSE[..10]], Some(fails(io::ErrorKind::Interrupted)));
        transport.reads.push_back(Ok(RESPONSE[10..].to_vec()));
        let response = exchange(&mut transport, &request(), Limits::default()).unwrap();
        assert_eq!(response.body, b"hello");

        // A reset, and what a socket's read timeout yields: passed on, never retried.
        for kind in [io::ErrorKind::ConnectionReset, io::ErrorKind::WouldBlock] {
            let mut transport = Script::of(&[&RESPONSE[..10]], Some(fails(kind)));
            let error = exchange(&mut transport, &request(), Limits::default()).unwrap_err();
            assert!(
                matches!(&error, Error::Io(io) if io.kind() == kind),
                "{error:?}"
            );
        }
    }

    #[test]
    fn malformed_response_is_an_http_error() {
        let mut transport = Script::of(&[b"HTTP/1.1 200 OK\nContent-Length: 0\n\n"], None);
        let error = exchange(&mut transport, &request(), Limits::default()).unwrap_err();
        assert!(
            matches!(error, Error::Http(HttpError::Malformed(_))),
            "{error:?}"
        );
    }

    #[test]
    fn response_to_head_is_not_read_past_its_head() {
        let mut head = request();
        head.method = b"HEAD".to_vec();
        let wire = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n";
        let mut transport = Script::of(&[wire], None);
        let response = exchange(&mut transport, &head, Limits::default()).unwrap();
        assert_eq!(response.body, b"");
        assert!(response.keep_alive);
    }
}
