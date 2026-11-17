//! What the encoders produce, checked against an independent implementation.

mod contract {
    use uri::form::{ByteSet, Encoding, Space};

    fn encode(encoding: Encoding, input: &[u8]) -> String {
        let mut output = Vec::new();
        encoding.component(input, &mut output);
        String::from_utf8(output).unwrap()
    }

    /// Expected values from Python 3.15.2: `urllib.parse.quote_plus(input, safe='')`.
    #[test]
    fn query_encoding_matches_pythons_quote_plus() {
        let cases: [(&[u8], &str); 8] = [
            (b"", ""),
            (b"abc-._~XYZ019", "abc-._~XYZ019"),
            (b"a b", "a+b"),
            (b"a+b&c=d", "a%2Bb%26c%3Dd"),
            (b"*", "%2A"),
            (
                b"/?#[]@!$'(),;:",
                "%2F%3F%23%5B%5D%40%21%24%27%28%29%2C%3B%3A",
            ),
            (b"\x00\x7f\x80\xff", "%00%7F%80%FF"),
            ("caf\u{e9} \u{4e2d}".as_bytes(), "caf%C3%A9+%E4%B8%AD"),
        ];
        for (input, expected) in cases {
            assert_eq!(encode(Encoding::QUERY, input), expected, "{input:?}");
        }
    }

    #[test]
    fn space_can_be_written_as_a_percent_escape() {
        let encoding = Encoding {
            keep: ByteSet::UNRESERVED,
            space: Space::Percent,
        };
        assert_eq!(encode(encoding, b"a b"), "a%20b");
    }

    /// The other common convention keeps `*` and encodes `~`. The caller states it; nothing
    /// here guesses.
    #[test]
    fn kept_bytes_are_whatever_the_caller_says() {
        let encoding = Encoding {
            keep: ByteSet::UNRESERVED.with(b"*").without(b"~"),
            space: Space::Plus,
        };
        assert_eq!(encode(encoding, b"a*b~c d"), "a*b%7Ec+d");
        let literal_space = Encoding {
            keep: ByteSet::EMPTY.with(b" "),
            space: Space::Plus,
        };
        assert_eq!(
            encode(literal_space, b"a b"),
            "%61 %62",
            "a kept space stays a space"
        );
    }

    #[test]
    fn pairs_are_joined_in_the_order_given() {
        let pairs: [(&[u8], &[u8]); 4] = [(b"b", b"2"), (b"a", b"1 2"), (b"a", b""), (b"", b"x")];
        assert_eq!(Encoding::QUERY.pairs(pairs), b"b=2&a=1+2&a=&=x");
        assert_eq!(Encoding::QUERY.pairs(std::iter::empty()), b"");
    }

    /// Every byte either stays itself, becomes `+`, or becomes `%` and two uppercase
    /// hexadecimal digits that name it.
    #[test]
    fn every_byte_encodes_to_something_that_decodes_back() {
        for byte in 0..=u8::MAX {
            let encoded = encode(Encoding::QUERY, &[byte]);
            let decoded = match encoded.as_bytes() {
                [b'+'] => b' ',
                [b'%', high, low] => {
                    assert!(!high.is_ascii_lowercase() && !low.is_ascii_lowercase());
                    u8::from_str_radix(&encoded[1..], 16).unwrap()
                }
                [kept] => {
                    assert!(ByteSet::UNRESERVED.contains(*kept));
                    *kept
                }
                other => panic!("byte {byte:#04x} encoded as {other:?}"),
            };
            assert_eq!(decoded, byte);
        }
    }

    #[test]
    fn unreserved_is_letters_digits_and_four_marks() {
        let count = (0..=u8::MAX)
            .filter(|&byte| ByteSet::UNRESERVED.contains(byte))
            .count();
        assert_eq!(count, 26 + 26 + 10 + 4);
        assert!(!ByteSet::EMPTY.contains(b'a'));
        assert!(
            !ByteSet::UNRESERVED.contains(0x80 | b'a'),
            "no high-bit aliasing"
        );
    }
}
