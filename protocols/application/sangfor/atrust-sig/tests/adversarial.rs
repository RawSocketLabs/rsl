//! Hostile input: the values these functions take come from a server that has not been
//! authenticated yet, so none may crash, hang or run for long on what it sends.

mod adversarial {
    use crypto::RandomSource;
    use sangfor_atrust_sig::{Error, NonceGenerator, canonical_string, encrypt_secret};

    struct Ones;

    impl RandomSource for Ones {
        fn fill_bytes(&mut self, output: &mut [u8]) -> crypto::Result<()> {
            output.fill(1);
            Ok(())
        }
    }

    /// A 1 inside `depth` levels of `open` and `close`.
    fn nested(open: &str, close: &str, depth: usize) -> String {
        [open.repeat(depth), "1".to_owned(), close.repeat(depth)].concat()
    }

    #[test]
    fn anti_mitm_data_may_nest_sixteen_levels_and_no_deeper() {
        let malformed = Err(Error::Malformed("anti-MITM data"));
        // The outer object is the first level.
        let objects = |depth: usize| nested(r#"{"a":"#, "}", depth);
        assert_eq!(canonical_string(&objects(16)).unwrap(), "a:1");
        assert_eq!(canonical_string(&objects(17)), malformed);
        let arrays = |depth: usize| format!(r#"{{"a":{}}}"#, nested("[", "]", depth));
        assert_eq!(canonical_string(&arrays(15)).unwrap(), "a:1");
        assert_eq!(canonical_string(&arrays(16)), malformed);
    }

    #[test]
    fn deeply_nested_anti_mitm_data_is_refused_without_exhausting_the_stack() {
        let malformed = Err(Error::Malformed("anti-MITM data"));
        let deep = format!(r#"{{"a":{}}}"#, nested("[", "]", 100_000));
        assert_eq!(canonical_string(&deep), malformed);
        let deep = nested(r#"{"a":"#, "}", 100_000);
        assert_eq!(canonical_string(&deep), malformed);
    }

    #[test]
    fn invalid_json_is_malformed_wherever_the_fault_is() {
        let malformed = Err(Error::Malformed("anti-MITM data"));
        for json in [
            r#"{"a":}"#,
            r#"{"a":1,}"#,
            r#"{"a":"\ud800"}"#,
            r#"{"a":{"b":tru}}"#,
            r#"{"a":[1,]}"#,
            r#"{"a":1} trailing"#,
            "{\"a\":\"unterminated}",
        ] {
            assert_eq!(canonical_string(json), malformed, "{json}");
        }
    }

    #[test]
    fn nonce_generator_accepts_any_state_without_overflowing_or_sticking() {
        for state in [0, 1, 2_147_483_646, 2_147_483_647, 2_147_483_648, u64::MAX] {
            let mut generator = NonceGenerator::from_state(state);
            let first = generator.next_nonce();
            assert_eq!(first.len(), 64, "state {state}");
            assert_ne!(first, generator.next_nonce(), "state {state}");
        }
        // A state is taken modulo 2^31 - 1, where the recurrence keeps it.
        let mut wrapped = NonceGenerator::from_state(2_147_483_647 + 5);
        assert_eq!(
            wrapped.next_nonce(),
            NonceGenerator::from_state(5).next_nonce()
        );
    }

    #[test]
    fn oversized_public_key_is_refused_before_it_is_used() {
        // 8192 bits is the most accepted; one byte more is refused without any arithmetic.
        let too_long = "ff".repeat(1025);
        assert_eq!(
            encrypt_secret(&too_long, "65537", "secret", "rand", &mut Ones),
            Err(Error::Malformed("public key modulus"))
        );
        let huge = "ff".repeat(1 << 20);
        assert_eq!(
            encrypt_secret(&huge, "65537", "secret", "rand", &mut Ones),
            Err(Error::Malformed("public key modulus"))
        );
    }
}
