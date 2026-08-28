//! Published HKDF-SHA-384 evidence from Project Wycheproof.
//!
//! RFC 5869 publishes vectors only for SHA-256 and SHA-1, so the independent Wycheproof suite is
//! the published evidence for this hash. Provenance: `tests/vectors/hkdf-sha384/README.md`.

use rsl_crypto::{
    CryptoError,
    kdf::hkdf::sha384::{HkdfSha384Prk, derive, extract},
};

use super::wycheproof_fixtures::CASES;

fn decode(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("fixture is hex"))
        .collect()
}

/// Published evidence: every Wycheproof result is reproduced; invalid cases request more than
/// 255 blocks and must be rejected as `OutputTooLong`.
#[test]
fn wycheproof_results_are_reproduced() {
    let mut valid = 0;
    for case in &CASES {
        let ikm = decode(case.ikm);
        let salt = decode(case.salt);
        let info = decode(case.info);
        let mut output = vec![0_u8; case.size];
        let outcome = derive(Some(&salt), &ikm, &info, &mut output);
        if case.result == "valid" {
            outcome.unwrap_or_else(|e| panic!("tcId {} ({}): {e}", case.tc_id, case.comment));
            assert_eq!(output, decode(case.okm), "tcId {} okm", case.tc_id);
            let prk = extract(Some(&salt), &ikm).unwrap();
            let mut staged = vec![0_u8; case.size];
            prk.expand(&info, &mut staged).unwrap();
            assert_eq!(staged, output, "tcId {} staged", case.tc_id);
            valid += 1;
        } else {
            assert!(
                case.size > HkdfSha384Prk::MAX_OUTPUT_LEN,
                "tcId {}",
                case.tc_id
            );
            assert_eq!(
                outcome,
                Err(CryptoError::OutputTooLong),
                "tcId {}",
                case.tc_id
            );
        }
    }
    assert_eq!(
        valid, 80,
        "Wycheproof publishes 80 valid HKDF-SHA-384 cases"
    );
}
