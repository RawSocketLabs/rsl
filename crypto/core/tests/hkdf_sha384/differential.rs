//! Differential comparison with the independent `RustCrypto` HKDF-SHA-384 implementation.

use hkdf::Hkdf;
use rsl_crypto::kdf::hkdf::sha384::derive;
use sha2::Sha384 as ReferenceSha384;

/// Differential evidence over absent/present salts, varied context, and output boundaries.
#[test]
fn deterministic_inputs_match_rustcrypto() {
    for ikm_len in [0, 1, 22, 48, 80, 131] {
        let ikm = deterministic_bytes(ikm_len, 0x17);

        for salt_len in [None, Some(0), Some(13), Some(80)] {
            let salt = salt_len.map(|length| deterministic_bytes(length, 0x53));

            for output_len in [0, 1, 47, 48, 49, 96, 97, 255] {
                compare_case(salt.as_deref(), &ikm, output_len);
            }
        }
    }
}

/// Differential evidence at the exact 255-block maximum.
#[test]
fn maximum_output_matches_rustcrypto() {
    compare_case(Some(b"salt"), b"input keying material", 255 * 48);
}

fn compare_case(salt: Option<&[u8]>, ikm: &[u8], output_len: usize) {
    let info = deterministic_bytes(67, 0xa9);
    let reference = Hkdf::<ReferenceSha384>::new(salt, ikm);
    let mut expected = vec![0_u8; output_len];
    reference
        .expand(&info, &mut expected)
        .expect("the differential output length is within RFC limits");
    let mut actual = vec![0_u8; output_len];
    derive(salt, ikm, &info, &mut actual).expect("the input is within RFC limits");

    assert_eq!(actual, expected);
}

fn deterministic_bytes(length: usize, domain: u8) -> Vec<u8> {
    (0..length)
        .map(|index| {
            let index = u8::try_from(index % 256).expect("the remainder is at most 255");
            index.wrapping_mul(97).wrapping_add(domain)
        })
        .collect()
}
