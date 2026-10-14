//! Published RSASSA-PSS 2048/SHA-256 evidence from NIST CAVP and Project Wycheproof.

use rsl_crypto::{
    CryptoError,
    signature::{Verifier, rsa_pss::RsaPssSha256VerifyingKey},
};

use crate::{
    cavp_fixtures::{SIGGEN_CASES, SIGGEN_EXPONENT, SIGGEN_MODULUS, SIGVER_CASES},
    support,
    wycheproof_fixtures::{CASES as WYCHEPROOF_CASES, EXPONENT, MODULUS, SALT_LEN},
};

/// Published evidence: all 18 CAVP `SigVerPSS` 2048/SHA-256 verdicts are reproduced.
///
/// NIST used 32-byte salts for this group; a case whose `e` was changed may fail at import or
/// at verification, and both count as the printed `F`.
#[test]
fn cavp_sigver_pss_verdicts_are_reproduced() {
    let mut accepted = 0;
    for case in &SIGVER_CASES {
        assert_eq!(support::decode(case.salt).len(), 32, "group salt length");
        let outcome = RsaPssSha256VerifyingKey::from_components(
            support::decode(case.n),
            support::decode(case.e),
        )
        .and_then(|key| {
            key.verify_sha256(
                support::decode(case.message),
                &support::signature(case.signature),
            )
        });
        let expected_pass = case.verdict.starts_with('P');
        assert_eq!(
            outcome.is_ok(),
            expected_pass,
            "e ends {} verdict {}",
            &case.e[case.e.len() - 8..],
            case.verdict
        );
        if expected_pass {
            accepted += 1;
        }
    }
    assert_eq!(
        accepted, 3,
        "NIST publishes three passing cases in this group"
    );
}

/// Published evidence: all 10 CAVP `SigGenPSS` 2048/SHA-256 signatures verify with the 20-byte
/// salt length NIST used, and fail under the default 32-byte expectation.
#[test]
fn cavp_siggen_pss_signatures_verify_with_their_published_salt_length() {
    let key = RsaPssSha256VerifyingKey::from_components(
        support::decode(SIGGEN_MODULUS),
        support::decode(SIGGEN_EXPONENT),
    )
    .unwrap();
    for (index, case) in SIGGEN_CASES.iter().enumerate() {
        let salt_len = support::decode(case.salt).len();
        assert_eq!(salt_len, 20, "case {index} salt length");
        let message = support::decode(case.message);
        let signature = support::signature(case.signature);
        key.verify_sha256_with_salt_len(&message, &signature, salt_len)
            .unwrap_or_else(|_| panic!("case {index} verifies with sLen = 20"));
        assert_eq!(
            key.verify_sha256(&message, &signature),
            Err(CryptoError::InvalidSignature),
            "case {index} must not verify with sLen = 32"
        );
    }
}

/// Published evidence: every Wycheproof `rsa_pss_2048_sha256_mgf1_32` result is reproduced.
#[test]
fn wycheproof_results_are_reproduced() {
    assert_eq!(SALT_LEN, 32);
    let key = RsaPssSha256VerifyingKey::from_components(
        support::decode(MODULUS),
        support::decode(EXPONENT),
    )
    .unwrap();
    let mut valid = 0;
    for case in &WYCHEPROOF_CASES {
        let outcome = Verifier::verify(
            &key,
            &support::decode(case.message),
            &support::signature(case.signature),
        );
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
            valid += 1;
        } else {
            assert_eq!(outcome, Err(CryptoError::InvalidSignature));
        }
    }
    assert_eq!(
        valid, 63,
        "Wycheproof publishes 63 valid cases in this file"
    );
    assert!(
        WYCHEPROOF_CASES
            .iter()
            .any(|case| case.flags.contains("WrongPrimitive")),
        "the PKCS #1 v1.5 wrong-primitive case is present"
    );
}

/// Regression evidence: the digest path agrees with the message path.
#[test]
fn prehashed_verification_matches_the_message_path() {
    let key = RsaPssSha256VerifyingKey::from_components(
        support::decode(MODULUS),
        support::decode(EXPONENT),
    )
    .unwrap();
    let case = WYCHEPROOF_CASES
        .iter()
        .find(|case| case.result == "valid" && !case.message.is_empty())
        .unwrap();
    let message = support::decode(case.message);
    let digest = rsl_crypto::digest::sha2::sha256::Sha256::digest(&message).unwrap();
    key.verify_sha256_digest(&digest, &support::signature(case.signature))
        .unwrap();
    assert_eq!(key.signature_len(), 256);
    assert_eq!(
        format!("{key:?}"),
        "RsaPssSha256VerifyingKey { modulus_bits: 2048 }"
    );
}
