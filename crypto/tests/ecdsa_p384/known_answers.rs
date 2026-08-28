//! Published ECDSA P-384/SHA-384 evidence from RFC 6979 A.2.6 and NIST CAVP SigVer/SigGen.

use rsl_crypto::{
    CryptoError,
    signature::{Verifier, ecdsa_p384::EcdsaP384SigningKey},
};

use crate::{cavp_siggen_fixtures::CASES as SIGGEN_CASES, cavp_sigver_fixtures::CASES, support};

/// RFC 6979 A.2.6 public key `U = xG` for the published private key `x`.
const RFC_6979_UX: &str = "ec3a4e415b4e19a4568618029f427fa5da9a8bc4ae92e02e06aae5286b300c64def8f0ea9055866064a254515480bc13";
const RFC_6979_UY: &str = "8015d9b72d7d57244ea8ef9ac0c621896708a59367f9dfb9f54ca84b3f1c9db1288b231c3ae0d4fe7344fd2533264720";

/// RFC 6979 A.2.6, "With SHA-384, message = "sample"".
const SAMPLE_R: &str = "94edbb92a5ecb8aad4736e56c691916b3f88140666ce9fa73d64c4ea95ad133c81a648152e44acf96e36dd1e80fabe46";
const SAMPLE_S: &str = "99ef4aeb15f178cea1fe40db2603138f130e740a19624526203b6351d0a3a94fa329c145786e679e7b82c71a38628ac8";

/// RFC 6979 A.2.6, "With SHA-384, message = "test"".
const TEST_R: &str = "8203b63d3c853e8d77227fb377bcf7b7b772e97892a80f36ab775d509d7a5feb0542a7f0812998da8f1dd3ca3cf023db";
const TEST_S: &str = "ddd0760448d42d8a43af45af836fce4de8be06b485e9b61b827c2f13173923e06a739f040649a667bf3b828246baa5a5";

/// Published evidence: both RFC 6979 A.2.6 SHA-384 signatures verify under the published key.
#[test]
fn rfc_6979_sha384_signatures_verify() {
    let key = support::verifying_key(RFC_6979_UX, RFC_6979_UY);
    key.verify_sha384(b"sample", &support::signature(SAMPLE_R, SAMPLE_S))
        .expect("published 'sample' signature verifies");
    key.verify_sha384(b"test", &support::signature(TEST_R, TEST_S))
        .expect("published 'test' signature verifies");
}

/// Published-derived evidence: swapping the RFC 6979 messages fails verification.
#[test]
fn rfc_6979_signatures_do_not_verify_over_the_other_message() {
    let key = support::verifying_key(RFC_6979_UX, RFC_6979_UY);
    assert_eq!(
        key.verify_sha384(b"test", &support::signature(SAMPLE_R, SAMPLE_S)),
        Err(CryptoError::InvalidSignature)
    );
    assert_eq!(
        key.verify_sha384(b"sample", &support::signature(TEST_R, TEST_S)),
        Err(CryptoError::InvalidSignature)
    );
}

/// Published evidence: all 15 CAVP `SigVer` `[P-384,SHA-384]` verdicts are reproduced.
#[test]
fn cavp_sigver_verdicts_are_reproduced() {
    let mut accepted = 0;
    for case in &CASES {
        let key = support::verifying_key(case.x, case.y);
        let message = support::decode_vec(case.message);
        let result = key.verify_sha384(&message, &support::signature(case.r, case.s));
        let expected_pass = case.verdict.starts_with('P');
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
        "NIST publishes three passing P-384/SHA-384 cases"
    );
}

/// Regression evidence: the generic contract and the digest path agree with the message path.
#[test]
fn generic_and_prehashed_paths_match_the_message_path() {
    let key = support::verifying_key(RFC_6979_UX, RFC_6979_UY);
    let signature = support::signature(SAMPLE_R, SAMPLE_S);
    Verifier::verify(&key, b"sample", &signature).expect("generic dispatch verifies");
    let digest = rsl_crypto::digest::sha2::sha384::Sha384::digest(b"sample").unwrap();
    key.verify_sha384_digest(&digest, &signature)
        .expect("caller-computed digest verifies");
}

/// RFC 6979 A.2.6 private key `x`.
const RFC_6979_X: &str = "6b9d3dad2e1b8c1c05b19875b6659f4de23c3b667bf297ba9aa47740787137d896d5724e4c70a825f872c9ea60d2edf5";

/// Published evidence: deterministic signing reproduces RFC 6979 A.2.6's SHA-384 signatures
/// byte for byte and derives the published public key.
#[test]
fn rfc_6979_deterministic_signatures_are_reproduced_exactly() {
    let key = EcdsaP384SigningKey::from_bytes(support::decode(RFC_6979_X)).unwrap();
    assert_eq!(
        key.verifying_key().as_bytes(),
        &support::uncompressed(RFC_6979_UX, RFC_6979_UY)
    );
    assert_eq!(
        key.sign_sha384(b"sample").unwrap(),
        support::signature(SAMPLE_R, SAMPLE_S)
    );
    assert_eq!(
        key.sign_sha384(b"test").unwrap(),
        support::signature(TEST_R, TEST_S)
    );
}

/// Published evidence: every CAVP `SigGen` case's private key derives the published point, and
/// the published signature verifies. The published random `k` is checked white-box.
#[test]
fn cavp_siggen_keys_derive_published_points_and_signatures_verify() {
    for case in &SIGGEN_CASES {
        let signing = EcdsaP384SigningKey::from_bytes(support::decode(case.d)).unwrap();
        let verifying = signing.verifying_key();
        assert_eq!(
            verifying.as_bytes(),
            &support::uncompressed(case.qx, case.qy),
            "public point for d={}",
            case.d
        );
        let message = support::decode_vec(case.message);
        verifying
            .verify_sha384(&message, &support::signature(case.r, case.s))
            .unwrap_or_else(|_| panic!("published signature verifies for d={}", case.d));
        let ours = signing.sign_sha384(&message).unwrap();
        verifying
            .verify_sha384(&message, &ours)
            .expect("deterministic signature verifies");
        assert_ne!(
            ours,
            support::signature(case.r, case.s),
            "CAVP used a random k"
        );
    }
}
