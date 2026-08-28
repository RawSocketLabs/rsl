//! Raw bytes and structured mutations of published NIST PKITS certificates exercise strict X.509
//! parsing and an independent `x509-parser` comparison without changing the production graph.
//!
//! Run: `cargo +nightly fuzz run certificate_parse --fuzz-dir pki/fuzz`.
#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use rsl_x509::Certificate;
use x509_parser::parse_x509_certificate;

static FIXTURES: LazyLock<[Vec<u8>; 3]> = LazyLock::new(|| {
    [
        decode_hex(include_str!(
            "../../tests/vectors/nist-pkits/TrustAnchorRootCertificate.hex"
        )),
        decode_hex(include_str!(
            "../../tests/vectors/nist-pkits/GoodCACert.hex"
        )),
        decode_hex(include_str!(
            "../../tests/vectors/nist-pkits/ValidCertificatePathTest1EE.hex"
        )),
    ]
});

fn decode_hex(input: &str) -> Vec<u8> {
    let digits: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let (pairs, remainder) = digits.as_chunks::<2>();
    assert!(remainder.is_empty());
    pairs
        .iter()
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).unwrap();
            let low = (pair[1] as char).to_digit(16).unwrap();
            u8::try_from((high << 4) | low).unwrap()
        })
        .collect()
}

fn candidate(data: &[u8]) -> Vec<u8> {
    if data.first().is_some_and(|selector| selector & 0x80 != 0) {
        return data[1..].to_vec();
    }
    let selector = data.first().copied().unwrap_or(0);
    let mut certificate = FIXTURES[usize::from(selector) % FIXTURES.len()].clone();
    let (mutations, _) = data.get(1..).unwrap_or_default().as_chunks::<3>();
    for mutation in mutations.iter().take(64) {
        let index = usize::from(u16::from_le_bytes([mutation[0], mutation[1]])) % certificate.len();
        certificate[index] ^= mutation[2];
    }
    certificate
}

fuzz_target!(|data: &[u8]| {
    let encoded = candidate(data);
    let rsl = Certificate::from_der(&encoded);
    let independent = parse_x509_certificate(&encoded);

    if let Ok(certificate) = &rsl {
        assert_eq!(certificate.encoded(), encoded);
        assert_eq!(certificate.to_der(), encoded);
        assert!(!certificate.tbs_certificate().encoded().is_empty());
    }
    if let (Ok(certificate), Ok((remaining, independent))) = (&rsl, &independent) {
        assert!(remaining.is_empty());
        assert_eq!(certificate.encoded(), independent.as_ref());
        assert_eq!(
            certificate.tbs_certificate().encoded(),
            independent.tbs_certificate.as_ref()
        );
        let reference_serial = independent.tbs_certificate.raw_serial();
        let reference_magnitude = reference_serial
            .strip_prefix(&[0])
            .unwrap_or(reference_serial);
        assert_eq!(
            certificate.tbs_certificate().serial_number(),
            reference_magnitude
        );
    }
});
