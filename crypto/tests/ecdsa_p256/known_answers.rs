//! Published ECDSA P-256/SHA-256 evidence from RFC 6979 A.2.5 and NIST CAVP SigVer.

use rsl_crypto::{CryptoError, signature::Verifier};

use crate::{cavp_sigver_fixtures::CASES, support};

/// RFC 6979 A.2.5 public key `U = xG` for the published private key `x`.
const RFC_6979_UX: &str = "60FED4BA255A9D31C961EB74C6356D68C049B8923B61FA6CE669622E60F29FB6";
const RFC_6979_UY: &str = "7903FE1008B8BC99A41AE9E95628BC64F2F1B20C2D7E9F5177A3C294D4462299";

/// RFC 6979 A.2.5, "With SHA-256, message = "sample"".
const SAMPLE_R: &str = "EFD48B2AACB6A8FD1140DD9CD45E81D69D2C877B56AAF991C34D0EA84EAF3716";
const SAMPLE_S: &str = "F7CB1C942D657C41D436C7A1B6E29F65F3E900DBB9AFF4064DC4AB2F843ACDA8";

/// RFC 6979 A.2.5, "With SHA-256, message = "test"".
const TEST_R: &str = "F1ABB023518351CD71D881567B1EA663ED3EFCF6C5132B354F28D3B0B7D38367";
const TEST_S: &str = "019F4113742A2B14BD25926B49C649155F267E60D3814B4C0CC84250E46F0083";

/// Published evidence: both RFC 6979 A.2.5 SHA-256 signatures verify under the published key.
#[test]
fn rfc_6979_sha256_signatures_verify() {
    let key = support::verifying_key(RFC_6979_UX, RFC_6979_UY);
    key.verify_sha256(b"sample", &support::signature(SAMPLE_R, SAMPLE_S))
        .expect("published 'sample' signature verifies");
    key.verify_sha256(b"test", &support::signature(TEST_R, TEST_S))
        .expect("published 'test' signature verifies");
}

/// Published-derived evidence: swapping the RFC 6979 messages fails verification.
#[test]
fn rfc_6979_signatures_do_not_verify_over_the_other_message() {
    let key = support::verifying_key(RFC_6979_UX, RFC_6979_UY);
    assert_eq!(
        key.verify_sha256(b"test", &support::signature(SAMPLE_R, SAMPLE_S)),
        Err(CryptoError::InvalidSignature)
    );
    assert_eq!(
        key.verify_sha256(b"sample", &support::signature(TEST_R, TEST_S)),
        Err(CryptoError::InvalidSignature)
    );
}

/// Published evidence: all 15 CAVP SigVer `[P-256,SHA-256]` verdicts are reproduced.
#[test]
fn cavp_sigver_verdicts_are_reproduced() {
    let mut accepted = 0;
    for case in &CASES {
        let key = support::verifying_key(case.x, case.y);
        let message = support::decode_vec(case.message);
        let result = key.verify_sha256(&message, &support::signature(case.r, case.s));
        let expected_pass = case.verdict.starts_with("P");
        assert_eq!(
            result.is_ok(),
            expected_pass,
            "Qx={} verdict {}",
            case.x,
            case.verdict
        );
        if expected_pass {
            accepted += 1;
        } else {
            assert_eq!(result, Err(CryptoError::InvalidSignature));
        }
    }
    assert_eq!(
        accepted, 3,
        "NIST publishes three passing P-256/SHA-256 cases"
    );
}

/// Regression evidence: the generic contract and the digest path agree with the message path.
#[test]
fn generic_and_prehashed_paths_match_the_message_path() {
    let key = support::verifying_key(RFC_6979_UX, RFC_6979_UY);
    let signature = support::signature(SAMPLE_R, SAMPLE_S);
    Verifier::verify(&key, b"sample", &signature).expect("generic dispatch verifies");
    let digest = rsl_crypto::digest::sha2::sha256::Sha256::digest(b"sample").unwrap();
    key.verify_sha256_digest(&digest, &signature)
        .expect("caller-computed digest verifies");
}
