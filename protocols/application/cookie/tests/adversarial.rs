//! Hostile `Set-Cookie` values: whatever a server sends, the jar stays bounded and nothing
//! panics.

mod adversarial {
    use cookie::{Cookie, Jar};

    const NOW: i64 = 1_000_000_000;
    /// Bytes the parsers give meaning to, and two they do not.
    const ALPHABET: &[u8] = b"=;-/ :0123456789aexpirsmgth\t\xff";

    #[test]
    fn headers_the_algorithm_says_to_ignore_store_nothing() {
        let ignored: [&[u8]; 6] = [
            b"",
            b"no-equals-sign",
            b"=value",
            b"  =value",
            b";",
            b"; a=b",
        ];
        for header in ignored {
            assert_eq!(
                Cookie::parse(header, b"/", NOW),
                None,
                "{}",
                header.escape_ascii()
            );
        }
    }

    #[test]
    fn odd_but_parseable_headers_keep_what_they_say() {
        let cookie = |header: &[u8]| Cookie::parse(header, b"/", NOW).unwrap();
        let spaced = cookie(b"  name \t=  two words  ; Path = /x ; HttpOnly");
        assert_eq!(spaced.name, b"name");
        assert_eq!(spaced.value, b"two words");
        assert_eq!(spaced.path, b"/x");
        assert_eq!(
            cookie(b"a=b=c").value,
            b"b=c",
            "only the first equals sign splits"
        );
        assert_eq!(cookie(b"a=").value, b"");
        assert_eq!(cookie(b"a=1;;; ;Unknown;=;Expires=never").expires, None);
        assert_eq!(cookie(b"a=\xff\x00").value, b"\xff\x00", "values are bytes");
    }

    #[test]
    fn max_age_too_large_to_count_is_never_not_an_overflow() {
        let cookie = Cookie::parse(b"a=1; Max-Age=99999999999999999999999999", b"/", NOW);
        assert_eq!(cookie.unwrap().expires, Some(i64::MAX));
        let cookie = Cookie::parse(b"a=1; Max-Age=9223372036854775807", b"/", i64::MAX);
        assert_eq!(cookie.unwrap().expires, Some(i64::MAX));
        let cookie = Cookie::parse(b"a=1; Max-Age=-99999999999999999999999999", b"/", NOW);
        assert!(cookie.unwrap().is_expired(i64::MIN + 1));
    }

    #[test]
    fn oversized_cookie_is_not_stored() {
        let mut jar = Jar::new();
        // Name, value and path count: one byte of name, one of path, the rest value.
        let fits = [&b"a="[..], &vec![b'x'; Jar::MAX_COOKIE_LEN - 2]].concat();
        let too_long = [&b"b="[..], &vec![b'x'; Jar::MAX_COOKIE_LEN - 1]].concat();
        jar.store(&fits, b"/", NOW);
        jar.store(&too_long, b"/", NOW);
        assert_eq!(jar.cookies().len(), 1);
        assert_eq!(jar.cookies()[0].name, b"a");

        // A long path counts too, whether the header names it or the request supplies it.
        let long_path = [&b"/"[..], &vec![b'p'; Jar::MAX_COOKIE_LEN]].concat();
        jar.store(&[&b"c=1; Path="[..], &long_path].concat(), b"/", NOW);
        jar.store(b"d=1", &[&long_path[..], b"/page"].concat(), NOW);
        assert_eq!(jar.cookies().len(), 1);
    }

    #[test]
    fn cookie_that_would_break_a_header_is_not_stored() {
        let mut jar = Jar::new();
        for header in [
            &b"a=b\r\nX-Injected: 1"[..],
            b"a=b\nc",
            b"a=b\0c",
            b"a\rb=c",
            b"a=b; Path=/x\r\ny",
        ] {
            jar.store(header, b"/", NOW);
            assert_eq!(jar.cookies(), [], "{}", header.escape_ascii());
        }
        // The raw way in stores what it is given.
        jar.insert(Cookie::new("a", "b\r\nc"));
        assert_eq!(jar.header(b"/", NOW), Some(b"a=b\r\nc".to_vec()));
    }

    #[test]
    fn jar_drops_its_oldest_cookie_when_full() {
        let mut jar = Jar::new();
        for index in 0..=Jar::MAX_COOKIES {
            jar.store(format!("c{index}=v").as_bytes(), b"/", NOW);
        }
        assert_eq!(jar.cookies().len(), Jar::MAX_COOKIES);
        assert_eq!(jar.get(b"c0", NOW), None, "the oldest went");
        assert_eq!(jar.get(b"c1", NOW), Some(&b"v"[..]));
        assert!(
            jar.get(format!("c{}", Jar::MAX_COOKIES).as_bytes(), NOW)
                .is_some()
        );

        // Replacing a held cookie is not growth.
        jar.store(b"c1=again", b"/", NOW);
        assert_eq!(jar.cookies().len(), Jar::MAX_COOKIES);
        assert_eq!(jar.get(b"c1", NOW), Some(&b"again"[..]));
    }

    #[test]
    fn deleting_names_only_the_cookie_with_that_name_and_path() {
        let mut jar = Jar::new();
        jar.store(b"sid=a; Path=/", b"/", NOW);
        jar.store(b"sid=b; Path=/api", b"/", NOW);
        jar.store(b"sid=; Path=/api; Max-Age=0", b"/", NOW);
        assert_eq!(jar.cookies().len(), 1);
        assert_eq!(jar.cookies()[0].path, b"/");
        jar.store(b"other=; Max-Age=0", b"/", NOW);
        assert_eq!(
            jar.cookies().len(),
            1,
            "deleting what is not there changes nothing"
        );
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        let mut jar = Jar::new();
        let mut state = 0x2545_f491_u32;
        for _ in 0..2000 {
            let length = (state >> 24) as usize % 48;
            let header: Vec<u8> = (0..length)
                .map(|_| {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    // Lean towards the bytes the parser gives meaning to.
                    ALPHABET[(state >> 16) as usize % ALPHABET.len()]
                })
                .collect();
            let now = i64::from(state);
            jar.store(&header, &header, now);
            // The same bytes where a date and a number are expected.
            jar.store(&[&b"d=1; Expires="[..], &header].concat(), b"/", now);
            jar.store(&[&b"m=1; Max-Age="[..], &header].concat(), b"/", now);
            let _ = jar.header(&header, now);
        }
        assert!(jar.cookies().len() <= Jar::MAX_COOKIES);
    }
}
