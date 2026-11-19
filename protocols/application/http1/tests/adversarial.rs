//! Hostile and malformed input: a server's bytes are untrusted, so every case here must be a
//! clean error (or a preserved value), never a panic, a hang, or a guess about where a
//! message ends.

mod adversarial {
    use http1::{ChunkedDecoder, Error, Limits, Request, ResponseDecoder};

    #[test]
    fn fixed_length_validation_checks_the_fields_that_will_be_written() {
        let request =
            Request::post("example.test", "/path?q=1", "text/plain", b"abc".to_vec()).unwrap();
        assert!(request.validate_fixed_length().is_ok());
        assert!(
            Request::get("example.test", "/")
                .unwrap()
                .validate_fixed_length()
                .is_ok()
        );
        for (name, value) in [
            ("Space Before Colon ", "value"),
            ("X-Test", "invalid\rvalue"),
            ("Content-Length", "3"),
            ("Transfer-Encoding", "chunked"),
        ] {
            let mut raw = request.clone();
            raw.headers.push(name, value);
            assert!(raw.validate_fixed_length().is_err(), "{name}");
        }
        let mut raw = request.clone();
        raw.version.extend(b" ");
        assert!(raw.validate_fixed_length().is_err());
        let mut raw = request.clone();
        raw.method.extend(b" ");
        assert!(raw.validate_fixed_length().is_err());
        for length in [
            "0",
            "4",
            "3,3",
            "+3",
            "9999999999999999999999999999999999999",
        ] {
            let mut raw = Request::get("example.test", "/").unwrap();
            raw.headers.push("Content-Length", length);
            raw.body.clone_from(&request.body);
            assert!(raw.validate_fixed_length().is_err(), "{length}");
        }
    }

    /// Decodes `wire` as the response to a `GET` under `limits`.
    fn decode_with(wire: &[u8], limits: Limits) -> Result<Vec<u8>, Error> {
        let mut decoder = ResponseDecoder::new(b"GET", limits);
        decoder.receive(wire)?;
        decoder.receive_eof(true)?;
        Ok(decoder.finish()?.body)
    }

    fn decode(wire: &[u8]) -> Result<Vec<u8>, Error> {
        decode_with(wire, Limits::default())
    }

    /// Whether `result` is a refusal of the head's syntax.
    fn is_malformed(result: &Result<Vec<u8>, Error>) -> bool {
        matches!(result, Err(Error::Malformed(_)))
    }

    #[test]
    fn head_syntax_that_parsers_disagree_about_is_refused() {
        let cases: [(&str, &[u8]); 11] = [
            (
                "lone LF line endings",
                b"HTTP/1.1 200 OK\nContent-Length: 0\n\n",
            ),
            (
                "lone LF on one line",
                b"HTTP/1.1 200 OK\r\nA: b\nContent-Length: 0\r\n\r\n",
            ),
            ("lone CR in a value", b"HTTP/1.1 200 OK\r\nA: b\rc\r\n\r\n"),
            ("folded field", b"HTTP/1.1 200 OK\r\nA: b\r\n c\r\n\r\n"),
            (
                "space before the colon",
                b"HTTP/1.1 200 OK\r\nContent-Length : 0\r\n\r\n",
            ),
            ("NUL in a value", b"HTTP/1.1 200 OK\r\nA: b\0c\r\n\r\n"),
            ("no colon", b"HTTP/1.1 200 OK\r\nContent-Length 0\r\n\r\n"),
            ("empty name", b"HTTP/1.1 200 OK\r\n: 0\r\n\r\n"),
            ("two-digit status", b"HTTP/1.1 20 OK\r\n\r\n"),
            ("NUL in the reason", b"HTTP/1.1 200 O\0K\r\n\r\n"),
            ("not HTTP", b"SSH-2.0-OpenSSH_9.9\r\n\r\n"),
        ];
        for (name, wire) in cases {
            let result = decode(wire);
            assert!(is_malformed(&result), "{name}: {result:?}");
            // The refusal does not depend on how the bytes arrive.
            for at in 0..wire.len() {
                let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
                let split = decoder
                    .receive(&wire[..at])
                    .and_then(|_| decoder.receive(&wire[at..]))
                    .and_then(|_| decoder.receive_eof(true));
                assert!(
                    matches!(split, Err(Error::Malformed(_))),
                    "{name} split at {at}"
                );
            }
        }
    }

    #[test]
    fn unusual_but_unambiguous_heads_are_preserved() {
        let wire =
            b"HTTP/2.7 599 \xffodd reason\r\nX-Unknown:\t padded \t\r\nContent-Length: 0\r\n\r\n";
        let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
        decoder.receive(wire).unwrap();
        let head = decoder.finish().unwrap().head;
        assert_eq!(head.version, (2, 7));
        assert_eq!(head.status, 599);
        assert_eq!(head.reason, b"\xffodd reason");
        assert_eq!(head.headers.get("x-unknown"), Some(&b"padded"[..]));

        // A status line with no reason and no space after the code is common enough to take.
        assert_eq!(
            decode(b"HTTP/1.1 200\r\nContent-Length: 0\r\n\r\n"),
            Ok(Vec::new())
        );
    }

    /// A response with these header fields and this body.
    fn response(fields: &str, body: &[u8]) -> Vec<u8> {
        [
            format!("HTTP/1.1 200 OK\r\n{fields}\r\n\r\n").as_bytes(),
            body,
        ]
        .concat()
    }

    #[test]
    fn content_length_that_is_not_one_decimal_number_is_refused() {
        let invalid = Err(Error::Framing(
            "Content-Length is not a single decimal number",
        ));
        let cases = [
            "Content-Length: 5\r\nContent-Length: 6",
            "Content-Length: 5, 6",
            "Content-Length: +5",
            "Content-Length: 5 5",
            "Content-Length: 0x5",
            "Content-Length: -5",
            "Content-Length: ",
            "Content-Length: ,",
            // Only space and tab are whitespace: a form feed is part of the value.
            "Content-Length: \x0c5",
            "Content-Length: 5\x0c",
            "Content-Length: 5,\x0c5",
        ];
        for fields in cases {
            assert_eq!(decode(&response(fields, b"hello")), invalid, "{fields:?}");
        }
        assert_eq!(
            decode(&response("Content-Length: 99999999999999999999", b"")),
            Err(Error::Framing("Content-Length overflows"))
        );
        // An empty list element is skipped, as the list syntax allows.
        assert_eq!(
            decode(&response("Content-Length: 5,", b"hello")),
            Ok(b"hello".to_vec())
        );
    }

    #[test]
    fn transfer_encoding_with_content_length_is_refused_even_when_the_body_is_chunked() {
        let chunked = b"5\r\nhello\r\n0\r\n\r\n";
        for fields in [
            "Transfer-Encoding: chunked\r\nContent-Length: 5",
            "Content-Length: 22\r\nTransfer-Encoding: chunked",
        ] {
            assert_eq!(
                decode(&response(fields, chunked)),
                Err(Error::Framing(
                    "both Transfer-Encoding and Content-Length are present"
                )),
                "{fields:?}"
            );
        }
    }

    #[test]
    fn transfer_coding_other_than_chunked_alone_is_unsupported() {
        let chunked = b"5\r\nhello\r\n0\r\n\r\n";
        let unsupported = Err(Error::Unsupported("a transfer coding other than chunked"));
        let cases = [
            "Transfer-Encoding: gzip, chunked",
            "Transfer-Encoding: chunked, gzip",
            "Transfer-Encoding: chunked\r\nTransfer-Encoding: gzip",
            "Transfer-Encoding: gzip\r\nTransfer-Encoding: chunked",
            "Transfer-Encoding: chunked, chunked",
            "Transfer-Encoding: \x0cchunked",
            "Transfer-Encoding: chunked\x0c",
            "Transfer-Encoding: ",
        ];
        for fields in cases {
            assert_eq!(
                decode(&response(fields, chunked)),
                unsupported,
                "{fields:?}"
            );
        }
    }

    #[test]
    fn what_this_crate_cannot_decode_is_reported_as_unsupported() {
        let unsupported = |wire: &[u8], method: &[u8]| {
            let mut decoder = ResponseDecoder::new(method, Limits::default());
            matches!(decoder.receive(wire), Err(Error::Unsupported(_)))
        };
        let switching = b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: x\r\n\r\n";
        let tunnel = b"HTTP/1.1 200 Connection established\r\n\r\n";
        assert!(unsupported(switching, b"GET"));
        assert!(unsupported(tunnel, b"CONNECT"));
        assert!(!unsupported(tunnel, b"GET"));
    }

    #[test]
    fn malformed_chunking_is_refused() {
        let chunked = |body: &[u8]| {
            let mut wire = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
            wire.extend(body);
            decode(&wire)
        };
        let cases: [(&str, &[u8]); 9] = [
            ("no size", b"\r\nhello\r\n0\r\n\r\n"),
            ("two sizes", b"5 5\r\nhello\r\n0\r\n\r\n"),
            ("size overflows after zeros", b"0010000000000000000\r\n"),
            ("size is not hexadecimal", b"5g\r\nhello\r\n0\r\n\r\n"),
            ("size line ends in a lone LF", b"5\nhello\r\n0\r\n\r\n"),
            ("data is not followed by CRLF", b"5\r\nhelloX\r\n0\r\n\r\n"),
            ("data is followed by a lone LF", b"5\r\nhello\n0\r\n\r\n"),
            ("size overflows", b"10000000000000000\r\n"),
            ("trailer line ends in a lone LF", b"0\r\nA: b\n\r\n"),
        ];
        for (name, body) in cases {
            let result = chunked(body);
            assert!(
                matches!(result, Err(Error::Framing(_))),
                "{name}: {result:?}"
            );
        }
        assert_eq!(
            chunked(b"0\r\n\r\n"),
            Ok(Vec::new()),
            "an empty body is fine"
        );
        assert_eq!(chunked(b"5\r\nhello\r\n0\r\n"), Err(Error::Truncated));
    }

    #[test]
    fn chunk_padding_extensions_and_trailers_are_bounded() {
        for prefix in [&b"1;"[..], b"0\r\n", b"1 ", b"0"] {
            let mut decoder = ChunkedDecoder::new(1024);
            let mut output = Vec::new();
            let filler = if prefix == b"0" {
                b'0'
            } else if prefix == b"1 " {
                b' '
            } else {
                b'x'
            };
            let mut wire = prefix.to_vec();
            wire.resize(wire.len() + 9000, filler);
            let result = decoder.decode(&wire, &mut output);
            assert_eq!(
                result,
                Err(Error::Framing("chunk extensions or trailers are too long")),
                "{prefix:?}"
            );
        }
    }

    #[test]
    fn chunked_decoder_stays_failed_after_an_error() {
        let mut decoder = ChunkedDecoder::new(4);
        let mut output = Vec::new();
        assert_eq!(
            decoder.decode(b"5\r\n", &mut output),
            Err(Error::BodyTooLarge)
        );
        assert_eq!(
            decoder.decode(b"hello\r\n0\r\n\r\n", &mut output),
            Err(Error::InvalidState)
        );
        assert!(!decoder.is_done());
    }

    #[test]
    fn draining_chunks_does_not_reset_the_body_limit() {
        let mut decoder = ChunkedDecoder::new(3);
        let mut output = Vec::new();
        decoder.decode(b"2\r\nab\r\n", &mut output).unwrap();
        assert_eq!(std::mem::take(&mut output), b"ab");
        decoder.decode(b"1\r\nc\r\n", &mut output).unwrap();
        assert_eq!(std::mem::take(&mut output), b"c");
        assert_eq!(decoder.clone().decode(b"0\r\n\r\n", &mut output), Ok(5));
        assert_eq!(
            decoder.decode(b"1\r\n", &mut output),
            Err(Error::BodyTooLarge)
        );
        assert_eq!(decoder.decode(b"d", &mut output), Err(Error::InvalidState));
        assert_eq!(output, b"");
    }

    #[test]
    fn chunked_output_buffer_remains_bounded_when_the_caller_adds_bytes() {
        let mut output = vec![0; 3];
        let mut decoder = ChunkedDecoder::new(4);
        assert_eq!(
            decoder.decode(b"2\r\n", &mut output),
            Err(Error::BodyTooLarge)
        );
        let mut decoder = ChunkedDecoder::new(4);
        decoder.decode(b"1\r\n", &mut output).unwrap();
        output.push(1);
        assert_eq!(decoder.decode(b"a", &mut output), Err(Error::BodyTooLarge));
        assert_eq!(decoder.decode(b"a", &mut output), Err(Error::InvalidState));
        assert_eq!(output, [0, 0, 0, 1]);
    }

    #[test]
    fn each_limit_admits_exactly_its_size() {
        // Body by declared length.
        let limits = Limits::default().with_body(5);
        assert_eq!(
            decode_with(&response("Content-Length: 5", b"hello"), limits),
            Ok(b"hello".to_vec())
        );
        assert_eq!(
            decode_with(&response("Content-Length: 6", b"hello!"), limits),
            Err(Error::BodyTooLarge)
        );
        // Body by chunks: the second chunk is what crosses the limit.
        let fits = b"3\r\nhel\r\n2\r\nlo\r\n0\r\n\r\n";
        let over = b"3\r\nhel\r\n3\r\nlo!\r\n0\r\n\r\n";
        let chunked = "Transfer-Encoding: chunked";
        assert_eq!(
            decode_with(&response(chunked, fits), limits),
            Ok(b"hello".to_vec())
        );
        assert_eq!(
            decode_with(&response(chunked, over), limits),
            Err(Error::BodyTooLarge)
        );
        // Header fields: 128 by default.
        let with_fields = |count: usize| {
            let mut wire = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n".to_vec();
            wire.extend(b"A: b\r\n".repeat(count - 1));
            wire.extend(b"\r\n");
            decode(&wire)
        };
        assert_eq!(with_fields(128), Ok(Vec::new()));
        assert_eq!(with_fields(129), Err(Error::TooManyHeaders));
        // Head size: 64 KiB by default, counted through the blank line.
        let with_head_of = |size: usize| {
            let mut wire = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nX: ".to_vec();
            wire.resize(size - 4, b'a');
            wire.extend(b"\r\n\r\n");
            decode(&wire)
        };
        assert_eq!(with_head_of(64 * 1024), Ok(Vec::new()));
        assert_eq!(with_head_of(64 * 1024 + 1), Err(Error::HeadTooLarge));
        // The same when the head's last byte arrives on its own.
        let mut wire = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nX: ".to_vec();
        wire.resize(64 * 1024 - 4, b'a');
        wire.extend(b"\r\n\r\n");
        let (last, rest) = wire.split_last().unwrap();
        let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
        assert_eq!(decoder.receive(rest), Ok(rest.len()));
        assert_eq!(decoder.receive(&[*last]), Ok(1));
        assert!(decoder.is_complete());
    }

    #[test]
    fn refused_head_can_still_be_inspected() {
        let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
        let wire = b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: other\r\n\r\n";
        assert!(matches!(decoder.receive(wire), Err(Error::Unsupported(_))));
        let (head, _) = decoder.into_partial();
        assert_eq!(head.unwrap().headers.get("Upgrade"), Some(&b"other"[..]));
    }

    #[test]
    fn oversized_parts_fail_as_soon_as_their_size_is_known() {
        let small = Limits::default().with_body(8);

        // A declared length fails at the head, before any body arrives.
        let mut decoder = ResponseDecoder::new(b"GET", small);
        let head = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n";
        assert_eq!(decoder.receive(head), Err(Error::BodyTooLarge));

        // A chunk fails at its size line.
        let mut decoder = ResponseDecoder::new(b"GET", small);
        let wire = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n4\r\n";
        assert_eq!(decoder.receive(wire), Err(Error::BodyTooLarge));

        // A body with no length fails when it passes the limit.
        let wire = b"HTTP/1.1 200 OK\r\n\r\n123456789";
        assert_eq!(decode_with(wire, small), Err(Error::BodyTooLarge));
        assert_eq!(
            decode_with(&wire[..wire.len() - 1], small),
            Ok(b"12345678".to_vec())
        );
    }

    #[test]
    fn head_that_never_ends_is_cut_off_at_the_limit() {
        let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
        let mut wire = b"HTTP/1.1 200 OK\r\nX-Padding: ".to_vec();
        wire.resize(200 * 1024, b'a');
        assert_eq!(decoder.receive(&wire), Err(Error::HeadTooLarge));
        assert_eq!(decoder.receive(b"more"), Err(Error::InvalidState));
    }

    #[test]
    fn flood_of_interim_responses_is_cut_off() {
        let interim = b"HTTP/1.1 102 Processing\r\n\r\n";
        let last = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let flood = |count: usize| [interim.repeat(count).as_slice(), last].concat();
        assert_eq!(decode(&flood(8)), Ok(Vec::new()));
        assert_eq!(decode(&flood(9)), Err(Error::TooManyInterimResponses));
    }

    #[test]
    fn connection_ending_after_only_interim_responses_is_truncation() {
        let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
        decoder.receive(b"HTTP/1.1 100 Continue\r\n\r\n").unwrap();
        assert_eq!(decoder.receive_eof(true), Err(Error::Truncated));
    }

    #[test]
    fn guided_request_refuses_what_would_split_the_message() {
        let invalid =
            |result: Result<Request, Error>| matches!(result, Err(Error::InvalidRequest(_)));
        assert!(invalid(Request::get("example.test", "/a b")));
        assert!(invalid(Request::get("example.test", "/a\r\nX: y")));
        assert!(invalid(Request::get("example.test", "")));
        assert!(invalid(Request::get("example.test\r\nX: y", "/")));
        let request = Request::get("example.test", "/").unwrap();
        assert!(invalid(request.clone().header("X-A", "b\r\nX-C: d")));
        assert!(invalid(request.clone().header("X-A", "b\nc")));
        assert!(invalid(request.clone().header("X-A", "b\0c")));
        assert!(invalid(request.clone().header("X A", "b")));
        assert!(invalid(request.clone().header("X-A:", "b")));
        assert!(invalid(request.clone().header("", "b")));
        // Fields servers disagree about when repeated or combined.
        assert!(invalid(request.clone().header("host", "other.test")));
        let post = Request::post("example.test", "/", "text/plain", b"x".to_vec()).unwrap();
        assert!(invalid(post.clone().header("Content-Length", "0")));
        assert!(invalid(post.header("Transfer-Encoding", "chunked")));
        let chunked = request.header("Transfer-Encoding", "chunked").unwrap();
        assert!(invalid(chunked.header("content-length", "5")));
    }

    #[test]
    fn raw_request_fields_are_sent_exactly_as_given() {
        let mut request = Request::get("example.test", "/").unwrap();
        request.method = b"WEIRD METHOD".to_vec();
        request.version = b"HTTP/9.9".to_vec();
        request.headers.push("Host", "second.test");
        request.target = b"/a b\r\nInjected: yes".to_vec();
        request.headers.push("Bad Name", "line one\r\nline two");
        request.body = b"unannounced".to_vec();
        assert_eq!(
            request.encode(),
            b"WEIRD METHOD /a b\r\nInjected: yes HTTP/9.9\r\n\
              Host: example.test\r\n\
              Host: second.test\r\n\
              Bad Name: line one\r\nline two\r\n\
              \r\n\
              unannounced"
        );
    }
}
