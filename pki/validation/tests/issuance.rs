//! Standard-derived construction tests over the complete X.509 → crypto → validation path.

use rsl_asn1::{Encoder, ObjectIdentifier};
use rsl_crypto::signature::{
    ecdsa_p256::EcdsaP256SigningKey, ecdsa_p384::EcdsaP384SigningKey, ed448::Ed448SigningKey,
    ed25519::Ed25519SigningKey,
};
use rsl_pki::{
    ErrorKind, PathValidator, Purpose, RequiredKeyUsage,
    issuance::{
        EcdsaP256CertificateSigner, EcdsaP384CertificateSigner, Ed448CertificateSigner,
        Ed25519CertificateSigner,
    },
    verify_certificate_signature,
};
use rsl_x509::{
    Certificate, Time,
    builder::{
        CertificateBuilder, ExtendedKeyPurpose, ExtensionDer, NameBuilder, NameDer,
        SignatureAlgorithmDer, SubjectAltName, SubjectPublicKeyInfoDer,
    },
    oid,
};

fn time(year: u16) -> Time {
    Time::new(year, 1, 1, 0, 0, 0).unwrap()
}

fn assert_self_signed(der: &[u8]) {
    let certificate = Certificate::from_der(der).unwrap();
    assert_eq!(certificate.encoded(), der);
    assert_eq!(
        certificate.tbs_certificate().version(),
        rsl_x509::Version::V3
    );
    assert!(
        certificate
            .tbs_certificate()
            .extension(oid::BASIC_CONSTRAINTS)
            .unwrap()
            .basic_constraints()
            .unwrap()
            .unwrap()
            .ca
    );
    assert!(
        certificate
            .tbs_certificate()
            .extension(oid::KEY_USAGE)
            .unwrap()
            .key_usage()
            .unwrap()
            .unwrap()
            .key_cert_sign()
    );
    verify_certificate_signature(&certificate, &certificate).unwrap();
    let anchors = [certificate.clone()];
    let path = PathValidator::builder()
        .leaf(&certificate)
        .trust_anchors(&anchors)
        .at_time(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
        .validate()
        .unwrap();
    assert_eq!(path.certificates(), &[&certificate]);
}

#[test]
fn all_builtin_signers_construct_verifiable_certificates() {
    let name = NameDer::common_name("RSL test root").unwrap();

    let ed25519_key = Ed25519SigningKey::from_seed([1; 32]);
    let ed25519 = Ed25519CertificateSigner::new(&ed25519_key);
    let certificate =
        CertificateBuilder::certificate_authority(&[1], name.clone(), name.clone(), None)
            .unwrap()
            .validity(time(2026), time(2027))
            .unwrap()
            .subject_public_key_info(ed25519.subject_public_key_info().unwrap())
            .sign(&ed25519)
            .unwrap();
    assert_self_signed(certificate.as_bytes());

    let ed448_key = Ed448SigningKey::from_seed([2; 57]);
    let ed448 = Ed448CertificateSigner::new(&ed448_key);
    let certificate =
        CertificateBuilder::certificate_authority(&[2], name.clone(), name.clone(), None)
            .unwrap()
            .validity(time(2026), time(2027))
            .unwrap()
            .subject_public_key_info(ed448.subject_public_key_info().unwrap())
            .sign(&ed448)
            .unwrap();
    assert_self_signed(certificate.as_bytes());

    let mut p256_scalar = [0_u8; 32];
    p256_scalar[31] = 1;
    let p256_key = EcdsaP256SigningKey::from_bytes(p256_scalar).unwrap();
    let p256 = EcdsaP256CertificateSigner::new(&p256_key);
    let certificate =
        CertificateBuilder::certificate_authority(&[3], name.clone(), name.clone(), None)
            .unwrap()
            .validity(time(2026), time(2027))
            .unwrap()
            .subject_public_key_info(p256.subject_public_key_info().unwrap())
            .sign(&p256)
            .unwrap();
    assert_self_signed(certificate.as_bytes());

    let mut p384_scalar = [0_u8; 48];
    p384_scalar[47] = 1;
    let p384_key = EcdsaP384SigningKey::from_bytes(p384_scalar).unwrap();
    let p384 = EcdsaP384CertificateSigner::new(&p384_key);
    let certificate = CertificateBuilder::certificate_authority(&[4], name.clone(), name, Some(0))
        .unwrap()
        .validity(time(2026), time(2027))
        .unwrap()
        .subject_public_key_info(p384.subject_public_key_info().unwrap())
        .sign(&p384)
        .unwrap();
    assert_self_signed(certificate.as_bytes());
}

#[test]
fn guided_empty_subject_leaf_round_trips_and_validates() {
    let root_key = Ed25519SigningKey::from_seed([11; 32]);
    let leaf_key = Ed25519SigningKey::from_seed([12; 32]);
    let root_signer = Ed25519CertificateSigner::new(&root_key);
    let leaf_subject = Ed25519CertificateSigner::new(&leaf_key);
    let root_name = NameDer::common_name("RSL root").unwrap();
    let key_identifier = [0xa5; 20];

    let root_der = CertificateBuilder::certificate_authority(
        &[1],
        root_name.clone(),
        root_name.clone(),
        Some(0),
    )
    .unwrap()
    .validity(time(2026), time(2027))
    .unwrap()
    .subject_public_key_info(root_signer.subject_public_key_info().unwrap())
    .subject_key_identifier(&key_identifier)
    .unwrap()
    .sign(&root_signer)
    .unwrap();

    let leaf_der = CertificateBuilder::end_entity(&[2], root_name, NameDer::empty())
        .unwrap()
        .validity(time(2026), time(2027))
        .unwrap()
        .subject_public_key_info(leaf_subject.subject_public_key_info().unwrap())
        .subject_alt_names(&[SubjectAltName::DnsName("leaf.test")])
        .unwrap()
        .extended_key_usage(&[ExtendedKeyPurpose::ServerAuth])
        .unwrap()
        .authority_key_identifier(&key_identifier)
        .unwrap()
        .sign(&root_signer)
        .unwrap();

    let root = root_der.certificate().unwrap();
    let leaf = leaf_der.certificate().unwrap();
    let san = leaf
        .tbs_certificate()
        .extension(oid::SUBJECT_ALT_NAME)
        .unwrap();
    assert!(san.critical());
    let anchors = [root];
    let path = PathValidator::builder()
        .leaf(&leaf)
        .trust_anchors(&anchors)
        .at_time(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
        .purpose(Purpose::ServerAuth)
        .required_key_usage(RequiredKeyUsage::DigitalSignature)
        .dns_name("leaf.test")
        .validate()
        .unwrap();
    assert_eq!(path.certificates().len(), 2);
}

#[test]
fn guided_profiles_reject_invalid_inputs_and_duplicate_extensions() {
    let name = NameDer::common_name("Root").unwrap();
    let key = Ed25519SigningKey::from_seed([21; 32]);
    let signer = Ed25519CertificateSigner::new(&key);

    assert!(CertificateBuilder::end_entity(&[0], name.clone(), name.clone()).is_err());
    assert!(CertificateBuilder::end_entity(&[0, 1], name.clone(), name.clone()).is_err());
    assert!(CertificateBuilder::end_entity(&[1; 21], name.clone(), name.clone()).is_err());
    assert!(
        CertificateBuilder::certificate_authority(&[1], name.clone(), NameDer::empty(), None,)
            .is_err()
    );
    assert!(
        CertificateBuilder::end_entity(&[1], name.clone(), name.clone())
            .unwrap()
            .validity(time(2027), time(2026))
            .is_err()
    );
    assert!(
        CertificateBuilder::end_entity(&[1], name.clone(), name.clone())
            .unwrap()
            .validity(
                Time {
                    year: 2026,
                    month: 13,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                },
                time(2027),
            )
            .is_err()
    );

    let unsigned = CertificateBuilder::end_entity(&[1], name.clone(), NameDer::empty())
        .unwrap()
        .validity(time(2026), time(2027))
        .unwrap()
        .subject_public_key_info(signer.subject_public_key_info().unwrap());
    assert!(
        unsigned
            .build_tbs(rsl_x509::builder::SignatureAlgorithmDer::ed25519().unwrap())
            .is_err()
    );

    let duplicate = ExtensionDer::basic_constraints(false, None, true).unwrap();
    assert!(
        CertificateBuilder::end_entity(&[1], name.clone(), name.clone())
            .unwrap()
            .validity(time(2026), time(2027))
            .unwrap()
            .subject_public_key_info(signer.subject_public_key_info().unwrap())
            .raw_extension(duplicate)
            .is_err()
    );
    assert!(
        CertificateBuilder::certificate_authority(&[2], name.clone(), name.clone(), None,)
            .unwrap()
            .validity(time(2026), time(2027))
            .unwrap()
            .subject_public_key_info(signer.subject_public_key_info().unwrap())
            .key_usage(rsl_x509::builder::KeyUsages::DIGITAL_SIGNATURE)
            .is_err()
    );
    assert!(
        CertificateBuilder::end_entity(&[3], name.clone(), name)
            .unwrap()
            .validity(time(2026), time(2027))
            .unwrap()
            .subject_public_key_info(signer.subject_public_key_info().unwrap())
            .key_usage(rsl_x509::builder::KeyUsages::KEY_CERT_SIGN)
            .is_err()
    );
}

#[test]
fn typed_name_and_public_key_helpers_reject_invalid_wire_forms() {
    let common_name = ObjectIdentifier::from_arcs(oid::COMMON_NAME).unwrap();
    assert!(
        NameBuilder::new()
            .printable_attribute(&common_name, "not_printable")
            .is_err()
    );
    assert!(
        NameBuilder::new()
            .printable_attribute(&common_name, "RSL Root + 1")
            .unwrap()
            .build()
            .is_ok()
    );

    assert!(SubjectPublicKeyInfoDer::ecdsa_p256(&[0; 65]).is_err());
    assert!(SubjectPublicKeyInfoDer::ecdsa_p384(&[0; 97]).is_err());
}

#[test]
fn validity_uses_the_utc_and_generalized_time_boundary() {
    let name = NameDer::common_name("Time boundary").unwrap();
    let key = Ed25519SigningKey::from_seed([25; 32]);
    let signer = Ed25519CertificateSigner::new(&key);
    let der = CertificateBuilder::certificate_authority(&[1], name.clone(), name, None)
        .unwrap()
        .validity(time(2049), time(2050))
        .unwrap()
        .subject_public_key_info(signer.subject_public_key_info().unwrap())
        .sign(&signer)
        .unwrap();
    let certificate = der.certificate().unwrap();
    assert_eq!(
        certificate.tbs_certificate().validity().not_before,
        time(2049)
    );
    assert_eq!(
        certificate.tbs_certificate().validity().not_after,
        time(2050)
    );
}

#[test]
fn raw_critical_extension_is_preserved_but_does_not_bypass_validation() {
    let name = NameDer::common_name("Experimental root").unwrap();
    let key = Ed25519SigningKey::from_seed([31; 32]);
    let signer = Ed25519CertificateSigner::new(&key);
    let custom_oid = ObjectIdentifier::from_arcs(&[1, 3, 6, 1, 4, 1, 55555, 1]).unwrap();
    let mut value = Encoder::new();
    value.octet_string(b"experiment").unwrap();
    let extension = ExtensionDer::from_parts(custom_oid, true, value.finish()).unwrap();
    let der = CertificateBuilder::certificate_authority(&[1], name.clone(), name, None)
        .unwrap()
        .validity(time(2026), time(2027))
        .unwrap()
        .subject_public_key_info(signer.subject_public_key_info().unwrap())
        .raw_extension(extension)
        .unwrap()
        .sign(&signer)
        .unwrap();
    let certificate = der.certificate().unwrap();
    let anchors = [certificate.clone()];

    assert_eq!(
        PathValidator::builder()
            .leaf(&certificate)
            .trust_anchors(&anchors)
            .at_time(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
            .validate()
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedCriticalExtension
    );
}

#[test]
fn raw_spki_and_staged_signing_remain_available() {
    let key = Ed25519SigningKey::from_seed([41; 32]);
    let signer = Ed25519CertificateSigner::new(&key);
    let spki = signer.subject_public_key_info().unwrap();
    let imported = SubjectPublicKeyInfoDer::from_der(spki.as_bytes().to_vec()).unwrap();
    let name = NameDer::common_name("Remote signer").unwrap();
    let tbs = CertificateBuilder::custom(&[1], name.clone(), name)
        .unwrap()
        .validity(time(2026), time(2027))
        .unwrap()
        .subject_public_key_info(imported)
        .build_tbs(rsl_x509::builder::SignatureAlgorithmDer::ed25519().unwrap())
        .unwrap();
    let signature = key.sign(tbs.as_bytes()).unwrap();
    let der = tbs.finish(signature.as_bytes()).unwrap();
    let certificate = der.certificate().unwrap();
    verify_certificate_signature(&certificate, &certificate).unwrap();
    let anchors = [certificate.clone()];
    assert!(
        PathValidator::builder()
            .leaf(&certificate)
            .trust_anchors(&anchors)
            .at_time(Time::new(2026, 6, 1, 0, 0, 0).unwrap())
            .validate()
            .is_ok()
    );
}

#[test]
fn raw_signature_algorithm_is_preserved_without_becoming_supported() {
    let key = Ed25519SigningKey::from_seed([51; 32]);
    let signer = Ed25519CertificateSigner::new(&key);
    let custom_oid = ObjectIdentifier::from_arcs(&[1, 3, 6, 1, 4, 1, 55555, 2]).unwrap();
    let mut algorithm = Encoder::new();
    algorithm
        .sequence(|sequence| sequence.object_identifier(&custom_oid))
        .unwrap();
    let algorithm = SignatureAlgorithmDer::from_der(algorithm.finish()).unwrap();
    let original_name = NameDer::common_name("Experimental signer").unwrap();
    let imported_name = NameDer::from_der(original_name.into_bytes()).unwrap();
    let tbs = CertificateBuilder::custom(&[1], imported_name.clone(), imported_name)
        .unwrap()
        .validity(time(2026), time(2027))
        .unwrap()
        .subject_public_key_info(signer.subject_public_key_info().unwrap())
        .build_tbs(algorithm)
        .unwrap();
    let signature = key.sign(tbs.as_bytes()).unwrap();
    let der = tbs.finish(signature.as_bytes()).unwrap();
    let certificate = der.certificate().unwrap();

    assert_eq!(certificate.signature_algorithm().oid(), &custom_oid);
    assert_eq!(
        verify_certificate_signature(&certificate, &certificate)
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedAlgorithm
    );
}
