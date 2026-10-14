//! Published AES-256-GCM evidence from NIST `AES_GCM.pdf` and Project Wycheproof.

use rsl_crypto::{
    CryptoError,
    aead::{
        Aead,
        gcm::{Aes256Gcm, Aes256GcmKey, Aes256GcmNonce, Aes256GcmTag},
    },
};

use crate::{nist_fixtures::CASES as NIST_CASES, support, wycheproof_fixtures::CASES};

/// Published evidence: GCM-AES256 Examples 1–5 seal to the printed ciphertext and tag and open
/// back to the plaintext; a changed tag fails.
#[test]
fn nist_gcm_aes256_examples_one_through_five() {
    for case in &NIST_CASES {
        let algorithm = Aes256Gcm::new(Aes256GcmKey::new(support::decode_array(case.key)));
        let nonce = Aes256GcmNonce::new(support::decode_array(case.iv));
        let aad = support::decode(case.aad);
        let plaintext = support::decode(case.plaintext);
        let sealed = algorithm.seal(&nonce, &aad, &plaintext).unwrap();
        assert_eq!(
            sealed.ciphertext(),
            support::decode(case.ciphertext).as_slice(),
            "Example {} ciphertext",
            case.example
        );
        assert_eq!(
            sealed.tag().as_bytes().as_slice(),
            support::decode(case.tag).as_slice(),
            "Example {} tag",
            case.example
        );
        assert_eq!(
            algorithm
                .open(&nonce, &aad, sealed.ciphertext(), sealed.tag())
                .unwrap(),
            plaintext
        );
        let mut wrong = sealed.tag().into_bytes();
        wrong[15] ^= 1;
        assert_eq!(
            algorithm.open(&nonce, &aad, sealed.ciphertext(), &Aes256GcmTag::new(wrong)),
            Err(CryptoError::AuthenticationFailed)
        );
    }
}

/// Published evidence: every Wycheproof 256-bit-key result is reproduced. Nonce sizes other
/// than 96 bits are unrepresentable and count as rejected; valid cases are re-sealed byte-exact.
#[test]
fn wycheproof_results_are_reproduced() {
    let mut valid = 0;
    for case in &CASES {
        let algorithm = Aes256Gcm::new(Aes256GcmKey::new(support::decode_array(case.key)));
        let aad = support::decode(case.aad);
        let ciphertext = support::decode(case.ct);
        let outcome =
            Aes256GcmNonce::try_from(support::decode(case.iv).as_slice()).and_then(|nonce| {
                let tag = Aes256GcmTag::try_from(support::decode(case.tag).as_slice())?;
                Aead::open(&algorithm, &nonce, &aad, &ciphertext, &tag)
            });
        let expected_valid = case.result == "valid" && case.iv.len() == 24;
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
            let nonce = Aes256GcmNonce::new(support::decode_array(case.iv));
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
    assert_eq!(
        valid, 39,
        "Wycheproof publishes 39 valid 96-bit-nonce cases for 256-bit keys"
    );
}
