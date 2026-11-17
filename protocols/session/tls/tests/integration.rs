//! The certificate module against real certificates: built by `x509`, signed with `crypto`
//! keys through `pki`'s issuance adapters, then parsed and validated back through this crate.

mod support;

mod integration {
    use crypto::signature::{
        ed25519::Ed25519SigningKey,
        rsa_pss::{RsaPssSha256VerifyingKey, RsaPssSignature},
    };
    use pki::{ErrorKind, RequiredKeyUsage, issuance::Ed25519CertificateSigner};
    use tls::{
        CertificateError, PeerChain, ServerVerification,
        wire::{Certificate as CertificateMessage, ClientHello, ServerHello, ServerKeyExchange},
    };
    use x509::{
        Certificate, PublicKey, Time,
        builder::{CertificateBuilder, ExtendedKeyPurpose, KeyUsages, NameDer, SubjectAltName},
    };

    use super::support::{self, hex};

    const ROOT: u8 = 1;
    const OTHER_ROOT: u8 = 2;
    const INTERMEDIATE: u8 = 3;
    const LEAF: u8 = 7;

    fn key(seed: u8) -> Ed25519SigningKey {
        Ed25519SigningKey::from_seed([seed; 32])
    }

    /// The subject name and key identifier of the CA whose key is seeded with `seed`.
    fn ca_name(seed: u8) -> (NameDer, [u8; 20]) {
        let name = NameDer::common_name(&format!("tls test ca {seed}")).unwrap();
        (name, [seed; 20])
    }

    fn time(year: u16) -> Time {
        Time::new(year, 6, 1, 0, 0, 0).unwrap()
    }

    /// A CA certificate for the key seeded `seed`, signed by the CA seeded `issuer`
    /// (self-signed when they are equal).
    fn ca_der(seed: u8, issuer: u8) -> Vec<u8> {
        let (subject_key, issuer_key) = (key(seed), key(issuer));
        let (subject, subject_id) = ca_name(seed);
        let (issuer_name, issuer_id) = ca_name(issuer);
        CertificateBuilder::certificate_authority(&[seed], issuer_name, subject, None)
            .unwrap()
            .validity(time(2026), time(2028))
            .unwrap()
            .subject_public_key_info(
                Ed25519CertificateSigner::new(&subject_key)
                    .subject_public_key_info()
                    .unwrap(),
            )
            .subject_key_identifier(&subject_id)
            .unwrap()
            .authority_key_identifier(&issuer_id)
            .unwrap()
            .sign(&Ed25519CertificateSigner::new(&issuer_key))
            .unwrap()
            .as_bytes()
            .to_vec()
    }

    /// A certificate for `vpn.test` and the `LEAF` key, issued by the CA seeded `issuer`.
    fn leaf_der(issuer: u8, purpose: ExtendedKeyPurpose, usages: KeyUsages) -> Vec<u8> {
        let (leaf_key, issuer_key) = (key(LEAF), key(issuer));
        let (issuer_name, issuer_id) = ca_name(issuer);
        CertificateBuilder::end_entity(&[0x40], issuer_name, NameDer::empty())
            .unwrap()
            .validity(time(2026), time(2028))
            .unwrap()
            .subject_public_key_info(
                Ed25519CertificateSigner::new(&leaf_key)
                    .subject_public_key_info()
                    .unwrap(),
            )
            .subject_alt_names(&[SubjectAltName::DnsName("vpn.test")])
            .unwrap()
            .extended_key_usage(&[purpose])
            .unwrap()
            .key_usage(usages)
            .unwrap()
            .authority_key_identifier(&issuer_id)
            .unwrap()
            .sign(&Ed25519CertificateSigner::new(&issuer_key))
            .unwrap()
            .as_bytes()
            .to_vec()
    }

    /// An ordinary server certificate issued by the CA seeded `issuer`.
    fn server_leaf_der(issuer: u8) -> Vec<u8> {
        leaf_der(
            issuer,
            ExtendedKeyPurpose::ServerAuth,
            KeyUsages::DIGITAL_SIGNATURE,
        )
    }

    /// Verifies `chain` for `dns_name` at `at`, trusting only the root seeded `anchor`, on
    /// behalf of a key exchange that needs a signing key.
    fn verify(
        chain: Vec<Vec<u8>>,
        anchor: u8,
        at: Time,
        dns_name: &str,
    ) -> Result<(), CertificateError> {
        let anchor_der = ca_der(anchor, anchor);
        let trust_anchors = [Certificate::from_der(&anchor_der).unwrap()];
        ServerVerification::Path {
            trust_anchors: &trust_anchors,
            time: at,
            dns_name,
        }
        .verify(
            &PeerChain::from_der(chain),
            RequiredKeyUsage::DigitalSignature,
        )
    }

    /// The `pki` failure category behind a rejected chain.
    fn path_error_kind(result: Result<(), CertificateError>) -> ErrorKind {
        match result {
            Err(CertificateError::Path(error)) => error.kind,
            other => panic!("expected a path-validation failure, got {other:?}"),
        }
    }

    #[test]
    fn leaf_issued_by_a_trust_anchor_for_the_requested_name_is_accepted() {
        let chain = vec![server_leaf_der(ROOT)];
        assert_eq!(verify(chain, ROOT, time(2027), "vpn.test"), Ok(()));
    }

    #[test]
    fn leaf_is_validated_through_the_intermediate_the_peer_sent() {
        let chain = vec![server_leaf_der(INTERMEDIATE), ca_der(INTERMEDIATE, ROOT)];
        assert_eq!(verify(chain, ROOT, time(2027), "vpn.test"), Ok(()));
    }

    #[test]
    fn leaf_whose_intermediate_was_not_sent_is_rejected() {
        let chain = vec![server_leaf_der(INTERMEDIATE)];
        let result = verify(chain, ROOT, time(2027), "vpn.test");
        assert_eq!(path_error_kind(result), ErrorKind::PathNotFound);
    }

    #[test]
    fn chain_for_another_name_is_rejected() {
        let chain = vec![server_leaf_der(ROOT)];
        let result = verify(chain, ROOT, time(2027), "other.test");
        assert_eq!(path_error_kind(result), ErrorKind::ServiceIdentityMismatch);
    }

    #[test]
    fn chain_from_an_untrusted_issuer_is_rejected() {
        let chain = vec![server_leaf_der(ROOT)];
        let result = verify(chain, OTHER_ROOT, time(2027), "vpn.test");
        assert_eq!(path_error_kind(result), ErrorKind::PathNotFound);
    }

    #[test]
    fn chain_outside_its_validity_period_is_rejected() {
        let chain = vec![server_leaf_der(ROOT)];
        let result = verify(chain, ROOT, time(2030), "vpn.test");
        assert_eq!(path_error_kind(result), ErrorKind::Expired);
    }

    #[test]
    fn leaf_issued_only_for_client_authentication_is_rejected() {
        let chain = vec![leaf_der(
            ROOT,
            ExtendedKeyPurpose::ClientAuth,
            KeyUsages::DIGITAL_SIGNATURE,
        )];
        let result = verify(chain, ROOT, time(2027), "vpn.test");
        assert_eq!(path_error_kind(result), ErrorKind::PurposeMismatch);
    }

    #[test]
    fn leaf_whose_key_may_not_sign_is_rejected_for_a_signing_key_exchange() {
        let chain = vec![leaf_der(
            ROOT,
            ExtendedKeyPurpose::ServerAuth,
            KeyUsages::KEY_ENCIPHERMENT,
        )];
        let result = verify(chain, ROOT, time(2027), "vpn.test");
        assert_eq!(path_error_kind(result), ErrorKind::RequiredKeyUsageMissing);
    }

    #[test]
    fn empty_chain_is_rejected_when_verifying() {
        assert_eq!(
            verify(Vec::new(), ROOT, time(2027), "vpn.test"),
            Err(CertificateError::EmptyChain)
        );
    }

    #[test]
    fn unparseable_certificate_is_reported_with_its_position() {
        let chain = vec![server_leaf_der(ROOT), vec![0xff, 0x00]];
        let result = verify(chain, ROOT, time(2027), "vpn.test");
        assert!(
            matches!(result, Err(CertificateError::Parse { index: 1, .. })),
            "{result:?}"
        );
    }

    #[test]
    fn unverified_accepts_a_chain_that_does_not_parse() {
        let chain = PeerChain::from_der(vec![vec![0xff, 0x00]]);
        assert_eq!(
            ServerVerification::Unverified.verify(&chain, RequiredKeyUsage::DigitalSignature),
            Ok(())
        );
        assert_eq!(chain.der(), [vec![0xff, 0x00]]);
    }

    #[test]
    fn leaf_public_key_is_the_key_the_leaf_was_issued_for() {
        let chain = PeerChain::from_der(vec![server_leaf_der(ROOT), ca_der(ROOT, ROOT)]);
        assert_eq!(
            chain.leaf_public_key(),
            Ok(PublicKey::Ed25519(key(LEAF).verifying_key().as_bytes()))
        );
    }

    /// The bytes an ECDHE server signs: both hello randoms, then its key-exchange parameters
    /// as sent (RFC 5246 §7.4.3, RFC 8422 §5.4).
    fn signed_key_exchange_input(exchange: &ServerKeyExchange) -> Vec<u8> {
        let client = ClientHello::decode_exact(&hex(support::CLIENT_HELLO)[4..]).unwrap();
        let server = ServerHello::decode_exact(&hex(support::SERVER_HELLO)[4..]).unwrap();
        let mut input = client.random.to_vec();
        input.extend(server.random);
        input.extend(exchange.params.to_bytes().unwrap());
        input
    }

    /// The whole decode path a client needs before it can trust a key exchange, on a real
    /// handshake: the `Certificate` message becomes a `PeerChain`, `x509` yields the leaf's
    /// RSA key, and `crypto` verifies OpenSSL's signature over the randoms and the re-encoded
    /// parameters. It fails if any wire type loses or reorders a byte the signature covers.
    #[test]
    fn openssl_key_exchange_signature_verifies_with_the_leaf_key_from_x509() {
        let message = CertificateMessage::decode_exact(&hex(support::CERTIFICATE)[4..]).unwrap();
        let chain = PeerChain::from(message);
        let PublicKey::Rsa { modulus, exponent } = chain.leaf_public_key().unwrap() else {
            panic!("the captured server certificate has an RSA key");
        };
        let key = RsaPssSha256VerifyingKey::from_components(modulus, exponent).unwrap();

        let mut exchange =
            ServerKeyExchange::decode_exact(&hex(support::SERVER_KEY_EXCHANGE)[4..]).unwrap();
        let signature = RsaPssSignature::from_bytes(exchange.signature.clone());
        key.verify_sha256(signed_key_exchange_input(&exchange), &signature)
            .expect("the server's signature covers the randoms and the parameters as sent");

        exchange.params.public[0] ^= 1;
        assert!(
            key.verify_sha256(signed_key_exchange_input(&exchange), &signature)
                .is_err(),
            "a changed server key must not verify"
        );
    }

    #[test]
    fn leaf_public_key_of_an_empty_chain_is_an_error() {
        assert_eq!(
            PeerChain::from_der(Vec::new()).leaf_public_key(),
            Err(CertificateError::EmptyChain)
        );
    }
}
