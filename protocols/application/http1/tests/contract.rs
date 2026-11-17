//! What a request looks like on the wire, and what the decoder makes of responses.
//!
//! RFC 9112 gives almost no byte-exact examples, so most responses here are synthetic: laid
//! out by hand from its grammar. One is captured from a real server. The test that anchors
//! framing is split invariance: however a response is cut across reads, the decoder must
//! produce the same response and use the same number of bytes.

mod contract {
    use http1::{BodyFraming, Error, Limits, Request, Response, ResponseDecoder, ResponseHead};

    /// Captured 2026-11-17 from `python3 -m http.server` (Python 3.15.2) on loopback, in
    /// reply to `GET /hello.txt`: an HTTP/1.0 response with a `Content-Length`.
    const PYTHON_HTTP_SERVER: &[u8] = b"HTTP/1.0 200 OK\r\n\
        Server: SimpleHTTP Python/3.15.2\r\n\
        Date: Tue, 17 Nov 2026 03:28:48 GMT\r\n\
        Content-type: text/plain\r\n\
        Content-Length: 6\r\n\
        Last-Modified: Tue, 17 Nov 2026 03:28:48 GMT\r\n\
        \r\n\
        hello\n";

    const WITH_LENGTH: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    /// Two chunks, the second with an extension, then a trailer field.
    const CHUNKED: &[u8] = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
        5\r\nhello\r\n6;note=x\r\n world\r\n0\r\nChecked: yes\r\n\r\n";
    const NO_CONTENT: &[u8] = b"HTTP/1.1 204 No Content\r\nContent-Length: 9\r\n\r\n";
    const CLOSING: &[u8] = b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 2\r\n\r\nok";
    const AFTER_INTERIM: &[u8] =
        b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
    const UNTIL_CLOSE: &[u8] = b"HTTP/1.1 200 OK\r\nServer: old\r\n\r\nall of it";

    /// The responses that end on their own, with the body each must decode to.
    const SELF_DELIMITED: [(&[u8], &[u8]); 6] = [
        (PYTHON_HTTP_SERVER, b"hello\n"),
        (WITH_LENGTH, b"hello"),
        (CHUNKED, b"hello world"),
        (NO_CONTENT, b""),
        (CLOSING, b"ok"),
        (AFTER_INTERIM, b"ok"),
    ];

    /// Feeds `pieces` to a decoder for a `GET`, in order, and returns the response and how
    /// many bytes it used, or the first error.
    fn decode(pieces: &[&[u8]]) -> Result<(Response, usize), Error> {
        let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
        let mut used = 0;
        for piece in pieces {
            used += decoder.receive(piece)?;
        }
        Ok((decoder.finish()?, used))
    }

    // --- Requests ---

    #[test]
    fn get_is_a_request_line_a_host_field_and_a_blank_line() {
        let request = Request::get("example.test:8443", "/a?b=c").unwrap();
        assert_eq!(
            request.encode(),
            b"GET /a?b=c HTTP/1.1\r\nHost: example.test:8443\r\n\r\n"
        );
    }

    #[test]
    fn post_carries_its_type_its_length_and_then_its_body() {
        let request = Request::post("example.test", "/login", "application/json", b"{}".to_vec())
            .unwrap()
            .header("X-Extra", " padded ")
            .unwrap();
        assert_eq!(
            request.encode(),
            b"POST /login HTTP/1.1\r\n\
              Host: example.test\r\n\
              Content-Type: application/json\r\n\
              Content-Length: 2\r\n\
              X-Extra: padded\r\n\
              \r\n\
              {}"
        );
    }

    // --- Responses ---

    #[test]
    fn each_response_decodes_to_its_body_and_uses_all_its_bytes() {
        for (wire, body) in SELF_DELIMITED {
            let (response, used) = decode(&[wire]).unwrap();
            assert_eq!(response.body, body);
            assert_eq!(used, wire.len());
        }
    }

    #[test]
    fn head_fields_are_a_view_of_the_bytes_received() {
        let (response, _) = decode(&[PYTHON_HTTP_SERVER]).unwrap();
        let head = &response.head;
        assert_eq!(head.version, (1, 0));
        assert_eq!(head.status, 200);
        assert_eq!(head.reason, b"OK");
        assert_eq!(head.headers.fields.len(), 5);
        assert_eq!(head.headers.get("content-TYPE"), Some(&b"text/plain"[..]));
        assert_eq!(head.headers.get("Missing"), None);
        assert_eq!(
            head.raw(),
            &PYTHON_HTTP_SERVER[..PYTHON_HTTP_SERVER.len() - 6]
        );
    }

    #[test]
    fn connection_can_be_reused_only_after_a_self_delimited_http_1_1_response() {
        let keep_alive = |wire: &[u8]| decode(&[wire]).unwrap().0.keep_alive;
        assert!(keep_alive(WITH_LENGTH));
        assert!(keep_alive(CHUNKED));
        assert!(keep_alive(NO_CONTENT));
        assert!(!keep_alive(CLOSING), "the server said it is closing");
        assert!(
            !keep_alive(PYTHON_HTTP_SERVER),
            "HTTP/1.0 closes by default"
        );
    }

    #[test]
    fn response_to_head_has_no_body_whatever_its_fields_say() {
        let mut decoder = ResponseDecoder::new(b"HEAD", Limits::default());
        assert_eq!(decoder.receive(WITH_LENGTH), Ok(WITH_LENGTH.len() - 5));
        assert_eq!(decoder.finish().unwrap().body, b"");
    }

    #[test]
    fn repeated_content_length_that_agrees_is_one_length() {
        let wire = b"HTTP/1.1 200 OK\r\nContent-Length: 2, 2\r\ncontent-length: 2\r\n\r\nok";
        assert_eq!(decode(&[wire]).unwrap().0.body, b"ok");
    }

    /// The head of `wire`. A `HEAD` response ends with its head whatever it says, which
    /// makes the decoder a way to get at any head.
    fn head_of(wire: &[u8]) -> ResponseHead {
        let mut decoder = ResponseDecoder::new(b"HEAD", Limits::default());
        decoder.receive(wire).unwrap();
        decoder.finish().unwrap().head
    }

    #[test]
    fn framing_follows_the_rfc_9112_order_of_precedence() {
        let framing = |method: &[u8], wire: &[u8]| BodyFraming::of_response(method, &head_of(wire));
        assert_eq!(framing(b"GET", WITH_LENGTH), Ok(BodyFraming::Length(5)));
        assert_eq!(framing(b"GET", CHUNKED), Ok(BodyFraming::Chunked));
        assert_eq!(framing(b"GET", UNTIL_CLOSE), Ok(BodyFraming::UntilClose));
        // Rule 1: no body for HEAD, 1xx, 204 or 304, whatever the fields say.
        assert_eq!(framing(b"HEAD", WITH_LENGTH), Ok(BodyFraming::None));
        assert_eq!(framing(b"GET", NO_CONTENT), Ok(BodyFraming::None));
        let not_modified = b"HTTP/1.1 304 Not Modified\r\nContent-Length: 5\r\n\r\n";
        assert_eq!(framing(b"GET", not_modified), Ok(BodyFraming::None));
        // Method names are case-sensitive: `head` is not HEAD, so its response has a body.
        assert_eq!(framing(b"head", WITH_LENGTH), Ok(BodyFraming::Length(5)));
        // Rule 2 is only for a successful CONNECT.
        let refused = b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 5\r\n\r\n";
        assert_eq!(framing(b"CONNECT", refused), Ok(BodyFraming::Length(5)));
        assert_eq!(framing(b"connect", WITH_LENGTH), Ok(BodyFraming::Length(5)));
    }

    #[test]
    fn chunked_is_recognised_in_any_letter_case() {
        let wire = b"HTTP/1.1 200 OK\r\ntransfer-encoding: CHUNKED\r\n\r\n2\r\nok\r\n0\r\n\r\n";
        assert_eq!(decode(&[wire]).unwrap().0.body, b"ok");
    }

    #[test]
    fn chunk_size_may_have_leading_zeros_and_trailing_space() {
        let wire = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
            00000000000000000002 \t\r\nok\r\n0\r\n\r\n";
        assert_eq!(decode(&[wire]).unwrap().0.body, b"ok");
    }

    #[test]
    fn only_http_1_1_and_later_1_x_keep_the_connection() {
        let keep_alive = |version: &str| {
            let wire = format!("HTTP/{version} 200 OK\r\nContent-Length: 0\r\n\r\n");
            decode(&[wire.as_bytes()]).unwrap().0.keep_alive
        };
        assert!(keep_alive("1.1"));
        assert!(keep_alive("1.2"));
        assert!(!keep_alive("1.0"));
        assert!(
            !keep_alive("2.0"),
            "another protocol's promises are not ours to make"
        );
    }

    // --- Split invariance ---

    #[test]
    fn response_decodes_the_same_however_it_is_split_across_reads() {
        for (wire, _) in SELF_DELIMITED {
            let whole = decode(&[wire]).unwrap();
            for at in 0..=wire.len() {
                let split = decode(&[&wire[..at], &wire[at..]]).unwrap();
                assert_eq!(split, whole, "split at {at}");
            }
            let bytes: Vec<&[u8]> = wire.chunks(1).collect();
            assert_eq!(decode(&bytes).unwrap(), whole, "one byte at a time");
        }
    }

    #[test]
    fn bytes_after_a_response_are_left_for_the_next_one() {
        for (first, first_body) in SELF_DELIMITED {
            for (second, second_body) in SELF_DELIMITED {
                let both = [first, second].concat();
                for at in 0..=both.len() {
                    let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
                    let mut used = decoder.receive(&both[..at]).unwrap();
                    used += decoder.receive(&both[used..]).unwrap();
                    assert_eq!(used, first.len(), "split at {at}");
                    assert_eq!(decoder.receive(b"more"), Ok(0), "complete: takes nothing");
                    assert_eq!(decoder.finish().unwrap().body, first_body);

                    let (response, used) = decode(&[&both[first.len()..]]).unwrap();
                    assert_eq!(
                        (response.body.as_slice(), used),
                        (second_body, second.len())
                    );
                }
            }
        }
    }

    #[test]
    fn every_strict_prefix_is_an_unfinished_response() {
        for (wire, _) in SELF_DELIMITED {
            for end in 0..wire.len() {
                let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
                assert_eq!(decoder.receive(&wire[..end]), Ok(end));
                assert!(!decoder.is_complete(), "{end} of {} bytes", wire.len());
                let expected = if end == 0 {
                    Error::ClosedBeforeResponse
                } else {
                    Error::Truncated
                };
                assert_eq!(decoder.receive_eof(true), Err(expected), "ended at {end}");
            }
        }
    }

    // --- A body that runs to the end of the connection ---

    #[test]
    fn body_without_a_length_ends_only_at_a_clean_close() {
        let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
        assert_eq!(decoder.receive(UNTIL_CLOSE), Ok(UNTIL_CLOSE.len()));
        assert!(!decoder.is_complete());
        assert_eq!(decoder.receive(b", and more"), Ok(10));
        assert_eq!(decoder.receive_eof(true), Ok(()));
        let response = decoder.finish().unwrap();
        assert_eq!(response.body, b"all of it, and more");
        assert!(!response.keep_alive);
    }

    #[test]
    fn body_without_a_length_is_refused_after_an_unconfirmed_close_unless_opted_in() {
        let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
        decoder.receive(UNTIL_CLOSE).unwrap();
        assert_eq!(decoder.receive_eof(false), Err(Error::UncleanClose));
        let (head, body) = decoder.into_partial();
        assert_eq!(head.map(|head| head.status), Some(200));
        assert_eq!(
            body, b"all of it",
            "the partial body is there for diagnosis"
        );

        let limits = Limits::default().accepting_unclean_close();
        let mut decoder = ResponseDecoder::new(b"GET", limits);
        decoder.receive(UNTIL_CLOSE).unwrap();
        assert_eq!(decoder.receive_eof(false), Ok(()));
        assert_eq!(decoder.finish().unwrap().body, b"all of it");
    }

    #[test]
    fn unfinished_response_cannot_be_taken() {
        let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
        decoder
            .receive(&WITH_LENGTH[..WITH_LENGTH.len() - 1])
            .unwrap();
        assert_eq!(decoder.finish().err(), Some(Error::InvalidState));
    }
}
