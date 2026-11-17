//! The golden vectors of the behaviour specification, `../../SPEC-atrust.md` §12, V1 to V8.
//!
//! Those vectors are *derived from the stated rule*: computed for the specification with
//! Python and OpenSSL on inputs chosen for it, by a method that first reproduced the values
//! the reference project's own tests expect. None was captured from a gateway. They show
//! that this crate computes what the specification says, not that a gateway agrees.

mod contract {
    use crypto::RandomSource;
    use crypto_legacy::rsa::{
        RsaPrivateKey,
        pkcs1v15::{Pkcs1v15PrivateOperations, RsaPkcs1v15Ciphertext},
    };
    use sangfor_atrust_sig::{
        Error, NonceGenerator, SignKey, canonical_string, certificate_digest, chain_matches,
        challenge_passes, der_digest, encrypt_secret, identifiers,
    };

    // The specification's shared inputs.
    const MODULUS: &str = "00C0FFEE5EED";
    const EXPONENT: &str = "65537";
    const CHALLENGE: &str = "example-challenge-0001";
    const DEVICE_ID: &str = "00112233445566778899aabbccddeeff";
    const ENCRYPTED_CHALLENGE: &str =
        "91F62C3D0672E10A14CF23E7C5F18EAEF19820E405852A2DF51DF478DD90B0ED";

    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .concat()
    }

    /// The sign key of V2.
    fn key() -> SignKey {
        SignKey::derive(MODULUS, EXPONENT, CHALLENGE).unwrap()
    }

    /// The sign key when the server sent no anti-MITM data.
    fn key_without_data() -> SignKey {
        SignKey::derive("", "", "").unwrap()
    }

    #[test]
    fn v1_challenge_check_passes_for_the_exact_uppercase_ciphertext_only() {
        let check = |encrypted: &str| challenge_passes(MODULUS, EXPONENT, CHALLENGE, encrypted);
        assert_eq!(check(ENCRYPTED_CHALLENGE), Ok(true));
        assert_eq!(check(&ENCRYPTED_CHALLENGE.to_lowercase()), Ok(false));
        assert_eq!(check(&ENCRYPTED_CHALLENGE[..62]), Ok(false));
        assert_eq!(check(""), Ok(false));
    }

    /// Spec §3.3: the check cannot pass when an input is empty. Each ciphertext below is
    /// what the arithmetic gives for that empty input (computed with Python `cryptography`,
    /// 2026-11-17), so only the rule, not a mismatch, can be what refuses it.
    #[test]
    fn challenge_check_refuses_an_empty_input_even_when_the_arithmetic_agrees() {
        let for_empty_modulus = "8320DE95968515FFA74485F1F65CE6610BE9C37D3BD27B14BA54582CB96C2856";
        let for_empty_exponent = "301A47FB46A1AC4E06849BE86C03266504DFF79B2FC2083573870A0992C9F4DC";
        let for_empty_challenge = "284FCA6BD3B017299D3BD40CAD39B564";
        assert_eq!(
            challenge_passes("", EXPONENT, CHALLENGE, for_empty_modulus),
            Ok(false)
        );
        assert_eq!(
            challenge_passes(MODULUS, "", CHALLENGE, for_empty_exponent),
            Ok(false)
        );
        assert_eq!(
            challenge_passes(MODULUS, EXPONENT, "", for_empty_challenge),
            Ok(false)
        );
    }

    /// A challenge that fills its last block exactly still gets a whole block of padding.
    /// Expected value from Python `cryptography`, 2026-11-17.
    #[test]
    fn challenge_of_exactly_one_block_is_padded_with_another() {
        let challenge = "0123456789abcdef";
        let encrypted = "310E5DB3EC449C6338B4FAE44FEF4EF09E3B0DB6EA343A2F97654B905EF03BA4";
        assert_eq!(
            challenge_passes(MODULUS, EXPONENT, challenge, encrypted),
            Ok(true)
        );
        let first_block_only = &encrypted[..32];
        assert_eq!(
            challenge_passes(MODULUS, EXPONENT, challenge, first_block_only),
            Ok(false)
        );
    }

    #[test]
    fn v2_sign_key_is_derived_from_the_device_key_strings_and_the_challenge() {
        assert_eq!(
            hex(key().expose_secret()),
            "E7B4C6CFA59121FDCB33ECC990676E9F0ACA451320B7DEB06A25D8EF1D980DE7"
        );
        assert_eq!(
            hex(key_without_data().expose_secret()),
            "6DCC2BEE653BAAB458A7CEE13821BD4A3D0B379603BD84AEF5E8070EAC5A3FD7"
        );
    }

    #[test]
    fn v3_mitm_sig_covers_the_sorted_scalars_of_the_object_as_received() {
        let object = concat!(
            r#"{"enable":1,"antiMITMRequest":false,"devicePubKeyMod":"00C0FFEE5EED","#,
            r#""devicePubKeyExp":"65537","rsaCert":"","sm2encCert":"","#,
            r#""challenge":"example-challenge-0001","#,
            r#""encryptedChallenge":"91F62C3D0672E10A14CF23E7C5F18EAEF19820E405852A2DF51DF478DD90B0ED","#,
            r#""ticket":"TK-0001","#,
            r#""mitmSig":"0e89931e17ea6ed910f9fd0e0491eb77b0f08f615ed4a7e97e696da8a04cf35f"}"#,
        );
        let canonical = canonical_string(object).unwrap();
        assert_eq!(
            canonical,
            concat!(
                "antiMITMRequest:false&challenge:example-challenge-0001&devicePubKeyExp:65537",
                "&devicePubKeyMod:00C0FFEE5EED&enable:1",
                "&encryptedChallenge:91F62C3D0672E10A14CF23E7C5F18EAEF19820E405852A2DF51DF478DD90B0ED",
                "&rsaCert:&sm2encCert:&ticket:TK-0001",
            )
        );
        let signature = "0E89931E17EA6ED910F9FD0E0491EB77B0F08F615ED4A7E97E696DA8A04CF35F";
        assert_eq!(key().sign(canonical.as_bytes()).unwrap(), signature);
        // A received signature is uppercased before it is compared.
        assert_eq!(
            key().verify(canonical.as_bytes(), &signature.to_lowercase()),
            Ok(true)
        );
        assert_eq!(key().verify(canonical.as_bytes(), ""), Ok(false));
        assert_eq!(
            key_without_data().verify(canonical.as_bytes(), signature),
            Ok(false)
        );
    }

    #[test]
    fn v4_nonce_generator_yields_the_specified_bytes() {
        let mut generator = NonceGenerator::from_state(20_261_116);
        assert_eq!(
            generator.next_nonce(),
            "8A5BF19A11717BF9A74BB450E2EEC26E882AC7339744AA3505344A94E544E7B8"
        );
        assert_ne!(
            generator.next_nonce(),
            generator.next_nonce(),
            "it advances"
        );
    }

    #[test]
    fn v5_certificate_digest_hashes_the_base64_text_then_a_constant() {
        let digest = "DAC223B577CE06F17C37839304EF8A94AE9EF3DA907EBFE09D6FB77B6E8771FB";
        assert_eq!(hex(&certificate_digest("MAMCAQU=")), digest);
        assert_eq!(hex(&der_digest(&[0x30, 0x03, 0x02, 0x01, 0x05])), digest);
    }

    #[test]
    fn v6_second_stage_response_must_carry_the_keys_signature_of_its_body() {
        let body = concat!(
            r#"{"code":0,"data":{"nonce":"#,
            r#""8A5BF19A11717BF9A74BB450E2EEC26E882AC7339744AA3505344A94E544E7B8","#,
            r#""antiMITMEnable":1}}"#,
        );
        let signature = "EE78499E113699CC1D5326246C2C260268E2F22D1E63A27EC9054A5721FD809A";
        assert_eq!(key().verify(body.as_bytes(), signature), Ok(true));
        let altered = body.replace("8A5B", "8A5C");
        assert_eq!(key().verify(altered.as_bytes(), signature), Ok(false));
    }

    #[test]
    fn v7_report_env_is_signed_over_path_query_and_body() {
        let message = concat!(
            "/controller/v1/public/reportEnv?clientType=SDPClient&lang=en-US&platform=Linux",
            r#"{"deviceId":"00112233445566778899aabbccddeeff","env":{"endpoint":{"device":{"type":"browser"},"#,
            r#""device_id":"00112233445566778899aabbccddeeff","mac_addresses":["02:00:5e:00:53:01"]}},"#,
            r#""ticket":"TK-0001"}"#,
        );
        assert_eq!(
            key_without_data().sign(message.as_bytes()).unwrap(),
            "E2FDAB67D324A0A6B4A64EC447DC93F782F451DC09EAE08395CF228AD198E140"
        );
    }

    #[test]
    fn v8_identifiers_have_the_specified_encodings() {
        assert_eq!(
            identifiers::rid("sdp.example.org:8443"),
            "c2RwLmV4YW1wbGUub3JnOjg0NDM="
        );
        assert_eq!(
            identifiers::env_header(DEVICE_ID),
            "eyJkZXZpY2VJZCI6IjAwMTEyMjMzNDQ1NTY2Nzc4ODk5YWFiYmNjZGRlZWZmIn0="
        );
        assert_eq!(
            identifiers::connection_id(DEVICE_ID, 1_794_787_200_000_000).unwrap(),
            "B4377F7BABF2991B7D6983C4D3E19CD4-1794787200000000"
        );
        let hashes = [
            (
                "/usr/bin/zju-connect",
                "51D7B8293B80675C346E3C78354EEB1979925693A276EA952CA2F04A6EF018A1",
            ),
            (
                "/usr/bin/google-chrome-stable",
                "AB49E7CA5E2BED4B3A7D9981456BE57B62CCE2D42C43C881256346D45581AF19",
            ),
            (
                "/usr/bin/ssh",
                "D0195B967DFD056E32DA2FDA9C53251423FF5032D7A06AF76050F96433D0DCC3",
            ),
        ];
        for (path, expected) in hashes {
            assert_eq!(identifiers::proc_hash(path).unwrap(), expected, "{path}");
        }
    }

    // --- Rules the specification states without a vector ---

    /// Spec §3.5: any certificate in the chain may be the one the server named, by either
    /// of its two base64 texts; an empty text names nothing.
    #[test]
    fn chain_matches_when_any_certificate_is_one_the_server_named() {
        let (named, other) = (
            [0x30, 0x03, 0x02, 0x01, 0x05],
            [0x30, 0x03, 0x02, 0x01, 0x06],
        );
        assert!(chain_matches(&[other, named], "MAMCAQU=", ""));
        assert!(chain_matches(&[named], "", "MAMCAQU="));
        assert!(!chain_matches(&[other], "MAMCAQU=", ""));
        assert!(!chain_matches(&[named], "", ""), "nothing was named");
        assert!(
            !chain_matches(&[] as &[[u8; 5]], "MAMCAQU=", ""),
            "nothing was presented"
        );
        // An empty text names no certificate, not the empty certificate.
        assert!(!chain_matches(&[[0_u8; 0]], "", ""));
        // The server's text is hashed as written, so an equivalent encoding does not match.
        assert!(!chain_matches(&[named], "MAMCAQU", ""));
    }

    /// Spec §3.3: nested scalars count, `mitmSig` is left out at every depth, and a number
    /// keeps the text it arrived with.
    #[test]
    fn canonical_string_flattens_nesting_and_keeps_numbers_as_written() {
        let object = r#"{"z":1.50,"a":{"m":null,"mitmSig":"x","k":[true,{"b":"qA\n"}]},"mitmSig":"y","e":1e3}"#;
        assert_eq!(
            canonical_string(object).unwrap(),
            "b:qA\n&e:1e3&k:true&m:null&z:1.50"
        );
        assert_eq!(canonical_string("{}").unwrap(), "");
        for not_an_object in ["", "[]", "1", "\"text\"", "{", "null"] {
            assert_eq!(
                canonical_string(not_an_object),
                Err(Error::Malformed("anti-MITM data")),
                "{not_an_object:?}"
            );
        }
    }

    /// The choices `canonical_string` documents for what the specification leaves open.
    #[test]
    fn canonical_string_orders_by_name_and_keeps_the_last_of_a_repeated_one() {
        // Bytewise by name: "a" < "a/" < "a1" < "b" < the two-byte "\u{e9}".
        let object = r#"{"é":5,"b":4,"a1":3,"a/":2,"a":1}"#;
        assert_eq!(
            canonical_string(object).unwrap(),
            "a:1&a/:2&a1:3&b:4&\u{e9}:5"
        );
        // A repeated name, a nested one, and array elements: the last in the text is kept.
        assert_eq!(canonical_string(r#"{"a":1,"a":2}"#).unwrap(), "a:2");
        assert_eq!(canonical_string(r#"{"a":1,"n":{"a":2}}"#).unwrap(), "a:2");
        assert_eq!(canonical_string(r#"{"n":{"a":2},"a":1}"#).unwrap(), "a:1");
        assert_eq!(canonical_string(r#"{"a":[1,2,3]}"#).unwrap(), "a:3");
        // A mitmSig member is left out whole; the name is matched exactly.
        assert_eq!(
            canonical_string(r#"{"mitmSig":{"a":1},"b":2}"#).unwrap(),
            "b:2"
        );
        assert_eq!(
            canonical_string(r#"{"MitmSig":"x","b":2}"#).unwrap(),
            "MitmSig:x&b:2"
        );
    }

    /// Spec §3.3: a state whose value less one is 0x7FFFFF00 or more yields no byte. From
    /// 247665088 the next state is 2^31 - 2, which is discarded, so the stream is the one
    /// that starts from 2^31 - 2 itself.
    #[test]
    fn nonce_generator_discards_states_at_the_top_of_its_range() {
        let mut skipping = NonceGenerator::from_state(247_665_088);
        let mut after = NonceGenerator::from_state(2_147_483_646);
        assert_eq!(skipping.next_nonce(), after.next_nonce());
    }

    #[test]
    fn nonce_generator_seeded_from_the_clock_never_starts_at_zero() {
        // 2^31 - 1 seconds reduces to zero, which the recurrence could never leave.
        let mut stuck = NonceGenerator::new(2_147_483_647);
        let mut one = NonceGenerator::from_state(1);
        assert_eq!(stuck.next_nonce(), one.next_nonce());
        assert_ne!(stuck.next_nonce(), "00".repeat(32));
    }

    /// A deterministic stand-in for a random source; never used outside tests.
    struct Counting(u8);

    impl RandomSource for Counting {
        fn fill_bytes(&mut self, output: &mut [u8]) -> crypto::Result<()> {
            for byte in output {
                self.0 = self.0.wrapping_add(1).max(1);
                *byte = self.0;
            }
            Ok(())
        }
    }

    /// A throwaway 1024-bit key, generated 2026-11-17 with Python `cryptography`.
    const TEST_MODULUS: &str = concat!(
        "c27a17863e6058c010396eb0867f93dc51753e09ba1b87bed5d3f76c16c6f5adb5974f49d93b8452",
        "3df495898e5734b6d750ccdd4be725153111265c57c854ff8d679658bb467aeee4ffcb3643219392",
        "f44bc1c80d90528ae7ac11654be448934a49008136c8ccf34625fca525955de6bec05da1a67c24b6",
        "f9164a062be8f431",
    );
    const TEST_PRIVATE_EXPONENT: &str = concat!(
        "a8d6988fca9d98cf7e066e8bc0e907b491dfc8b1672ab98a3b57e009ae28a368612bf1013b2089bb",
        "60e073e6cd75f664fabfc26c29968e423cac17f2b99f49f264164c1c1ebae333fa4d2b266d0e07fd",
        "42757df17607eb4b05c28a9bcdc5d665d46b6064d9d991c91f870961a6283da7451471d7502c29cf",
        "5c4abce16fbfa225",
    );

    fn unhex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
            .collect()
    }

    /// Spec §3.4, checked by decrypting with the matching private key: the plaintext is the
    /// secret, an underscore and the anti-replay value, cut into pieces of 11 bytes less than
    /// the modulus, one ciphertext block each, in lowercase hexadecimal.
    #[test]
    fn encrypted_secret_decrypts_to_the_secret_and_the_anti_replay_value() {
        let secret = "correct horse battery staple ".repeat(6); // longer than one block holds
        let encrypted = encrypt_secret(
            TEST_MODULUS,
            "65537",
            &secret,
            "rand-0001",
            &mut Counting(0),
        )
        .unwrap();
        assert_eq!(encrypted, encrypted.to_lowercase());

        let private =
            RsaPrivateKey::from_components(unhex(TEST_MODULUS), unhex(TEST_PRIVATE_EXPONENT))
                .unwrap();
        let blocks: Vec<u8> = unhex(&encrypted);
        let expected = format!("{secret}_rand-0001");
        assert_eq!(blocks.len(), 128 * expected.len().div_ceil(128 - 11));
        let plaintext: Vec<u8> = blocks
            .chunks(128)
            .flat_map(|block| {
                let block = RsaPkcs1v15Ciphertext::from_bytes(block.to_vec());
                private.decrypt_pkcs1v15(&block).unwrap()
            })
            .collect();
        assert_eq!(String::from_utf8(plaintext).unwrap(), expected);
    }

    /// Spec §3.4: a piece holds 11 bytes less than the modulus. With a 128-byte modulus, 117
    /// bytes of plaintext are one block and 118 are two.
    #[test]
    fn secret_is_cut_at_eleven_bytes_less_than_the_modulus() {
        let blocks = |plaintext_len: usize| {
            // The plaintext is the secret, an underscore, and a 4-byte anti-replay value.
            let secret = "s".repeat(plaintext_len - 5);
            let encrypted =
                encrypt_secret(TEST_MODULUS, "65537", &secret, "rand", &mut Counting(0)).unwrap();
            encrypted.len() / 256
        };
        assert_eq!(blocks(117), 1);
        assert_eq!(blocks(118), 2);
    }

    #[test]
    fn unusable_public_key_strings_are_reported() {
        let encrypt = |modulus: &str, exponent: &str| {
            encrypt_secret(modulus, exponent, "secret", "rand", &mut Counting(0))
        };
        assert_eq!(
            encrypt("", "65537"),
            Err(Error::Malformed("public key modulus"))
        );
        assert_eq!(
            encrypt("not hex", "65537"),
            Err(Error::Malformed("public key modulus"))
        );
        // An 11-byte modulus leaves no room for plaintext beside the padding.
        assert_eq!(
            encrypt(&"ff".repeat(11), "65537"),
            Err(Error::Malformed("public key modulus"))
        );
        // An even modulus is not an RSA modulus at all.
        assert_eq!(
            encrypt(&"fe".repeat(64), "65537"),
            Err(Error::Crypto(crypto::CryptoError::InvalidPublicKey))
        );
        assert_eq!(
            encrypt(TEST_MODULUS, "0"),
            Err(Error::Malformed("public key exponent"))
        );
        assert_eq!(
            encrypt(TEST_MODULUS, "0x10001"),
            Err(Error::Malformed("public key exponent"))
        );
        assert_eq!(
            encrypt(TEST_MODULUS, ""),
            Err(Error::Malformed("public key exponent"))
        );
    }

    #[test]
    fn made_up_identifiers_have_the_specified_shapes() {
        let device = identifiers::device_id(&mut Counting(0)).unwrap();
        assert_eq!(device, "0102030405060708090a0b0c0d0e0f10");
        // The first octet is forced to locally administered (bit 1) and unicast (bit 0).
        assert_eq!(
            identifiers::mac_address(&mut Counting(0)).unwrap(),
            "02:02:03:04:05:06"
        );
        assert_eq!(
            identifiers::mac_address(&mut Counting(0xfc)).unwrap(),
            "fe:fe:ff:01:02:03"
        );
        assert_eq!(
            identifiers::trace_id(&mut Counting(0xa0)).unwrap(),
            "a1a2a3a4"
        );
        let key = SignKey::generate(&mut Counting(0)).unwrap();
        assert_eq!(key.expose_secret()[..4], [1, 2, 3, 4]);
        assert_eq!(key.expose_secret()[31], 32);
    }
}
