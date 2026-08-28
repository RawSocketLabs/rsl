//! Published certificate-parser interoperability evidence.

use rsl_x509::{Certificate, ErrorKind, Time, Version};

fn decode_hex(input: &str) -> Vec<u8> {
    let digits: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    assert_eq!(digits.len() % 2, 0);
    digits
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).unwrap();
            let low = (pair[1] as char).to_digit(16).unwrap();
            u8::try_from((high << 4) | low).unwrap()
        })
        .collect()
}

fn fixture(name: &str) -> Vec<u8> {
    let encoded = match name {
        "root" => include_str!("../../tests/vectors/nist-pkits/TrustAnchorRootCertificate.hex"),
        "ca" => include_str!("../../tests/vectors/nist-pkits/GoodCACert.hex"),
        "leaf" => include_str!("../../tests/vectors/nist-pkits/ValidCertificatePathTest1EE.hex"),
        _ => panic!("unknown fixture"),
    };
    decode_hex(encoded)
}

#[test]
fn published_nist_pkits_valid_signature_chain_parses_exactly() {
    let root_der = fixture("root");
    let ca_der = fixture("ca");
    let leaf_der = fixture("leaf");
    let root = Certificate::from_der(&root_der).unwrap();
    let ca = Certificate::from_der(&ca_der).unwrap();
    let leaf = Certificate::from_der(&leaf_der).unwrap();

    assert_eq!(root.encoded(), root_der);
    assert_eq!(ca.to_der(), ca_der);
    assert_eq!(leaf.to_der(), leaf_der);
    assert_eq!(root.tbs_certificate().version(), Version::V3);
    assert_eq!(ca.tbs_certificate().version(), Version::V3);
    assert_eq!(leaf.tbs_certificate().version(), Version::V3);
    assert_eq!(leaf.tbs_certificate().serial_number(), [1]);
    assert_eq!(ca.tbs_certificate().serial_number(), [2]);
    assert_eq!(
        leaf.tbs_certificate().issuer().encoded(),
        ca.tbs_certificate().subject().encoded()
    );
    assert_eq!(
        ca.tbs_certificate().issuer().encoded(),
        root.tbs_certificate().subject().encoded()
    );
    assert_eq!(
        leaf.tbs_certificate().validity().not_before,
        Time::new(2010, 1, 1, 8, 30, 0).unwrap()
    );
    assert_eq!(
        leaf.tbs_certificate().validity().not_after,
        Time::new(2030, 12, 31, 8, 30, 0).unwrap()
    );

    // PKITS Test1 uses RSA with SHA-256, outside the deliberately narrow signature profile.
    // Syntax parsing and exact signed-byte preservation remain valid interoperability evidence.
    assert_eq!(
        leaf.signature_algorithm()
            .signature_algorithm()
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedAlgorithm
    );
}
