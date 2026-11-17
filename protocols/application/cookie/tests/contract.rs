//! The examples of RFC 6265 §3.1, and the storage and matching rules of §5.

mod contract {
    use cookie::{Cookie, Jar};

    /// An arbitrary present, between the two dates in the RFC's examples.
    const NOW: i64 = 1_000_000_000;

    fn header(jar: &Jar, path: &[u8]) -> Option<String> {
        jar.header(path, NOW)
            .map(|header| String::from_utf8(header).unwrap())
    }

    /// RFC 6265 §3.1, in the order the section gives its examples.
    #[test]
    fn rfc_6265_examples_produce_the_cookie_headers_it_shows() {
        let mut jar = Jar::new();
        jar.store(b"SID=31d4d96e407aad42", b"/", NOW);
        assert_eq!(header(&jar, b"/").as_deref(), Some("SID=31d4d96e407aad42"));

        let mut jar = Jar::new();
        jar.store(
            b"SID=31d4d96e407aad42; Path=/; Domain=example.com",
            b"/",
            NOW,
        );
        assert_eq!(
            header(&jar, b"/any/path").as_deref(),
            Some("SID=31d4d96e407aad42")
        );

        let mut jar = Jar::new();
        jar.store(b"SID=31d4d96e407aad42; Path=/; Secure; HttpOnly", b"/", NOW);
        jar.store(b"lang=en-US; Path=/; Domain=example.com", b"/", NOW);
        let both = Some("SID=31d4d96e407aad42; lang=en-US");
        assert_eq!(header(&jar, b"/").as_deref(), both);

        // An expiry in the future replaces the cookie and keeps its place.
        jar.store(
            b"lang=en-US; Expires=Wed, 09 Jun 2021 10:18:14 GMT",
            b"/",
            NOW,
        );
        assert_eq!(header(&jar, b"/").as_deref(), both);
        assert_eq!(jar.cookies()[1].expires, Some(1_623_233_894));

        // An expiry in the past removes it.
        jar.store(b"lang=; Expires=Sun, 06 Nov 1994 08:49:37 GMT", b"/", NOW);
        assert_eq!(header(&jar, b"/").as_deref(), Some("SID=31d4d96e407aad42"));
    }

    #[test]
    fn cookie_expires_at_its_time_and_not_before() {
        let mut jar = Jar::new();
        jar.store(b"a=1; Max-Age=60", b"/", NOW);
        assert_eq!(jar.cookies()[0].expires, Some(NOW + 60));
        assert_eq!(jar.header(b"/", NOW + 59), Some(b"a=1".to_vec()));
        assert_eq!(jar.header(b"/", NOW + 60), None);
        assert_eq!(jar.get(b"a", NOW + 59), Some(&b"1"[..]));
        assert_eq!(jar.get(b"a", NOW + 60), None);
    }

    #[test]
    fn max_age_wins_over_expires_and_zero_or_less_deletes() {
        let parse = |header: &[u8]| Cookie::parse(header, b"/", NOW).unwrap().expires;
        let expires = b"a=1; Expires=Wed, 09 Jun 2021 10:18:14 GMT";
        assert_eq!(parse(expires), Some(1_623_233_894));
        assert_eq!(
            parse(&[expires, &b"; Max-Age=10"[..]].concat()),
            Some(NOW + 10)
        );
        assert_eq!(
            parse(b"a=1; Max-Age=10; Expires=Wed, 09 Jun 2021 10:18:14 GMT"),
            Some(NOW + 10)
        );
        // The last of a repeated attribute wins; one that does not parse is skipped.
        assert_eq!(parse(b"a=1; Max-Age=10; Max-Age=20"), Some(NOW + 20));
        assert_eq!(parse(b"a=1; Max-Age=10; Max-Age=soon"), Some(NOW + 10));
        assert_eq!(parse(b"a=1; Max-Age=+5"), None, "a plus sign is no number");
        assert_eq!(parse(b"a=1; Max-Age="), None, "nor is nothing");
        assert_eq!(parse(b"a=1; Max-Age=-"), None, "nor is a lone minus sign");
        assert_eq!(parse(b"a=1; Max-Age=10; Max-Age=-"), Some(NOW + 10));

        for header in [
            &b"a=1; Max-Age=0"[..],
            b"a=1; Max-Age=-1",
            b"a=1; Max-Age=000",
        ] {
            let mut jar = Jar::new();
            jar.store(b"a=1", b"/", NOW);
            jar.store(header, b"/", NOW);
            assert_eq!(jar.cookies(), [], "{}", header.escape_ascii());
        }
    }

    /// RFC 6265 §5.2: attribute names match whatever their letter case.
    #[test]
    fn attribute_names_are_matched_without_regard_to_case() {
        let names = [
            ("PATH", "EXPIRES", "MAX-AGE"),
            ("path", "expires", "max-age"),
        ];
        for (path, expires, max_age) in names {
            let dated = format!("a=1; {path}=/x; {expires}=Wed, 09 Jun 2021 10:18:14 GMT");
            let cookie = Cookie::parse(dated.as_bytes(), b"/", NOW).unwrap();
            assert_eq!(cookie.path, b"/x", "{path}");
            assert_eq!(cookie.expires, Some(1_623_233_894), "{expires}");
            let aged = format!("a=1; {max_age}=7");
            let cookie = Cookie::parse(aged.as_bytes(), b"/", NOW).unwrap();
            assert_eq!(cookie.expires, Some(NOW + 7), "{max_age}");
        }
    }

    /// RFC 6265 §5.3: the last of a repeated attribute is the one that counts.
    #[test]
    fn last_of_a_repeated_path_or_expires_wins() {
        let cookie = |header: &[u8]| Cookie::parse(header, b"/por/x", NOW).unwrap();
        assert_eq!(cookie(b"a=1; Path=/one; Path=/two").path, b"/two");
        assert_eq!(
            cookie(b"a=1; Path=/one; Path=relative").path,
            b"/por",
            "back to the default"
        );
        let first = "Expires=Sun, 06 Nov 1994 08:49:37 GMT";
        let second = "Expires=Wed, 09 Jun 2021 10:18:14 GMT";
        let both = format!("a=1; {first}; {second}");
        assert_eq!(cookie(both.as_bytes()).expires, Some(1_623_233_894));
        let unparseable = format!("a=1; {first}; Expires=never");
        assert_eq!(cookie(unparseable.as_bytes()).expires, Some(784_111_777));
    }

    /// RFC 6265 §5.3: expired cookies go before a live one does, and a cookie set again
    /// after it expired is a new cookie, at the end of the order.
    #[test]
    fn expired_cookies_are_dropped_before_any_live_one() {
        let mut jar = Jar::new();
        jar.store(b"SID=live; Path=/", b"/", NOW);
        for index in 1..Jar::MAX_COOKIES {
            jar.store(format!("n{index}=x; Max-Age=1").as_bytes(), b"/", NOW);
        }
        jar.store(b"new=1", b"/", NOW + 3600);
        assert_eq!(jar.get(b"SID", NOW + 3600), Some(&b"live"[..]));
        assert_eq!(jar.cookies().len(), 2, "the expired ones are gone");

        let mut jar = Jar::new();
        jar.store(b"a=old; Max-Age=1", b"/", NOW);
        jar.store(b"b=1", b"/", NOW);
        jar.store(b"a=new", b"/", NOW + 2);
        assert_eq!(jar.header(b"/", NOW + 2), Some(b"b=1; a=new".to_vec()));
    }

    /// RFC 6265 §5.1.4: the default path is the request path up to its last slash.
    #[test]
    fn cookie_without_a_path_gets_the_requests_directory() {
        let path = |request: &[u8]| Cookie::parse(b"a=1", request, NOW).unwrap().path;
        assert_eq!(path(b"/"), b"/");
        assert_eq!(path(b"/login"), b"/");
        assert_eq!(path(b"/por/login_auth.csp?x=/y/z"), b"/por");
        assert_eq!(path(b"/a/b/"), b"/a/b");
        assert_eq!(path(b""), b"/");
        assert_eq!(path(b"relative/path"), b"/");
        // A Path attribute that does not start with a slash is as good as none.
        let given = |attribute: &[u8]| {
            let header = [&b"a=1; Path="[..], attribute].concat();
            Cookie::parse(&header, b"/por/x", NOW).unwrap().path
        };
        assert_eq!(given(b"/api"), b"/api");
        assert_eq!(given(b"api"), b"/por");
        assert_eq!(given(b""), b"/por");
    }

    /// RFC 6265 §5.1.4: identical, or a prefix that ends at a path-segment boundary.
    #[test]
    fn cookie_is_sent_only_under_its_path() {
        let cookie = |path: &[u8]| {
            let mut cookie = Cookie::new("a", "1");
            cookie.path = path.to_vec();
            cookie
        };
        assert!(cookie(b"/").matches_path(b"/anything"));
        assert!(cookie(b"/api").matches_path(b"/api"));
        assert!(cookie(b"/api").matches_path(b"/api/v1"));
        assert!(cookie(b"/api").matches_path(b"/api?query"));
        assert!(cookie(b"/api/").matches_path(b"/api/v1"));
        assert!(
            !cookie(b"/api").matches_path(b"/apiary"),
            "not a segment boundary"
        );
        assert!(!cookie(b"/api").matches_path(b"/"));
        assert!(!cookie(b"/api/").matches_path(b"/api"));
    }

    /// RFC 6265 §5.4: longer paths first, then older cookies first.
    #[test]
    fn header_lists_longer_paths_first_and_older_cookies_first() {
        let mut jar = Jar::new();
        jar.store(b"root1=a; Path=/", b"/", NOW);
        jar.store(b"deep=b; Path=/api/v1", b"/", NOW);
        jar.store(b"root2=c; Path=/", b"/", NOW);
        jar.store(b"mid=d; Path=/api", b"/", NOW);
        jar.store(b"other=e; Path=/elsewhere", b"/", NOW);
        assert_eq!(
            header(&jar, b"/api/v1/users").as_deref(),
            Some("deep=b; mid=d; root1=a; root2=c")
        );
        assert_eq!(header(&jar, b"/").as_deref(), Some("root1=a; root2=c"));
    }

    #[test]
    fn same_name_on_another_path_is_another_cookie() {
        let mut jar = Jar::new();
        jar.store(b"sid=outer; Path=/", b"/", NOW);
        jar.store(b"sid=inner; Path=/api", b"/", NOW);
        jar.store(b"sid=newer; Path=/", b"/", NOW);
        assert_eq!(
            header(&jar, b"/api").as_deref(),
            Some("sid=inner; sid=newer")
        );
        assert_eq!(
            jar.get(b"sid", NOW),
            Some(&b"newer"[..]),
            "the oldest slot, replaced"
        );
    }

    #[test]
    fn restored_cookie_applies_everywhere_and_never_expires() {
        let mut jar = Jar::new();
        jar.insert(Cookie::new("sid", "restored"));
        assert_eq!(
            jar.header(b"/deep/path", i64::MAX),
            Some(b"sid=restored".to_vec())
        );
    }
}
