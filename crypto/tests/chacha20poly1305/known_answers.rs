//! Published `AEAD_CHACHA20_POLY1305` evidence from RFC 8439 and Project Wycheproof.

use rsl_crypto::{
    CryptoError,
    aead::{
        Aead,
        chacha20poly1305::{
            ChaCha20Poly1305, ChaCha20Poly1305Key, ChaCha20Poly1305Nonce, ChaCha20Poly1305Tag,
        },
    },
    cipher::chacha20::{ChaCha20, ChaCha20Key, ChaCha20Nonce},
};

use crate::{
    rfc_fixtures::{APPENDIX_A5, KEY_GEN_CASES},
    support,
    wycheproof_fixtures::CASES as WYCHEPROOF_CASES,
};

/// Published evidence: RFC 8439 §2.8.2 seal output and round trip.
#[test]
fn section_2_8_2_seal_and_open() {
    let key = ChaCha20Poly1305Key::new(core::array::from_fn(|i| 0x80 + u8::try_from(i).unwrap()));
    let nonce = ChaCha20Poly1305Nonce::new(support::decode_array("070000004041424344454647"));
    let aad = support::decode("50515253c0c1c2c3c4c5c6c7");
    let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
    let expected_ciphertext = support::decode(
        "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b6116",
    );
    let expected_tag = support::decode_array("1ae10b594f09e26a7e902ecbd0600691");

    let algorithm = ChaCha20Poly1305::new(key);
    let sealed = algorithm.seal(&nonce, &aad, plaintext).unwrap();
    assert_eq!(sealed.ciphertext(), expected_ciphertext.as_slice());
    assert_eq!(sealed.tag().as_bytes(), &expected_tag);
    assert_eq!(
        algorithm
            .open(&nonce, &aad, sealed.ciphertext(), sealed.tag())
            .unwrap(),
        plaintext
    );
}

/// Published evidence: Appendix A.4's three Poly1305 key-generation vectors via block zero.
#[test]
fn appendix_a4_one_time_key_generation() {
    for (index, case) in KEY_GEN_CASES.iter().enumerate() {
        let cipher = ChaCha20::new(ChaCha20Key::new(support::decode_array(case.key)));
        let nonce = ChaCha20Nonce::new(support::decode_array(case.nonce));
        assert_eq!(
            &cipher.keystream_block(0, &nonce)[..32],
            support::decode(case.one_time_key).as_slice(),
            "A.4 vector {}",
            index + 1
        );
    }
}

/// Published evidence: Appendix A.5 decryption recovers the plaintext, and a changed tag fails.
#[test]
fn appendix_a5_decryption() {
    let algorithm = ChaCha20Poly1305::new(ChaCha20Poly1305Key::new(support::decode_array(
        APPENDIX_A5.key,
    )));
    let nonce = ChaCha20Poly1305Nonce::new(support::decode_array(APPENDIX_A5.nonce));
    let aad = support::decode(APPENDIX_A5.aad);
    let ciphertext = support::decode(APPENDIX_A5.ciphertext);
    let tag = ChaCha20Poly1305Tag::new(support::decode_array(APPENDIX_A5.tag));
    assert_eq!(
        algorithm.open(&nonce, &aad, &ciphertext, &tag).unwrap(),
        support::decode(APPENDIX_A5.plaintext)
    );
    let mut wrong = tag.into_bytes();
    wrong[0] ^= 1;
    assert_eq!(
        algorithm.open(&nonce, &aad, &ciphertext, &ChaCha20Poly1305Tag::new(wrong)),
        Err(CryptoError::AuthenticationFailed)
    );
}

/// Published evidence: every Wycheproof `chacha20_poly1305` result is reproduced. Nonce sizes
/// other than 96 bits are unrepresentable and count as rejected.
#[test]
fn wycheproof_results_are_reproduced() {
    let mut valid = 0;
    for case in &WYCHEPROOF_CASES {
        let algorithm =
            ChaCha20Poly1305::new(ChaCha20Poly1305Key::new(support::decode_array(case.key)));
        let aad = support::decode(case.aad);
        let ciphertext = support::decode(case.ct);
        let outcome = ChaCha20Poly1305Nonce::try_from(support::decode(case.iv).as_slice())
            .and_then(|nonce| {
                let tag = ChaCha20Poly1305Tag::try_from(support::decode(case.tag).as_slice())?;
                Aead::open(&algorithm, &nonce, &aad, &ciphertext, &tag)
            });
        let expected_valid = case.result == "valid";
        assert_eq!(
            outcome.is_ok(),
            expected_valid,
            "tcId {} ({}; flags {})",
            case.tc_id,
            case.comment,
            case.flags
        );
        if expected_valid {
            let plaintext = outcome.unwrap();
            assert_eq!(
                plaintext,
                support::decode(case.msg),
                "tcId {} plaintext",
                case.tc_id
            );
            let nonce = ChaCha20Poly1305Nonce::new(support::decode_array(case.iv));
            let sealed = algorithm.seal(&nonce, &aad, &plaintext).unwrap();
            assert_eq!(
                sealed.ciphertext(),
                ciphertext.as_slice(),
                "tcId {} seal",
                case.tc_id
            );
            assert_eq!(
                sealed.tag().as_bytes().as_slice(),
                support::decode(case.tag).as_slice(),
                "tcId {} tag",
                case.tc_id
            );
            valid += 1;
        }
    }
    assert_eq!(valid, 256, "Wycheproof publishes 256 valid cases");
}
