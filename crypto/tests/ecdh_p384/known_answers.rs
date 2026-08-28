//! Published ECDH P-384 evidence from RFC 5903 §8.2 and NIST CAVP ECC CDH primitive cases.

use rsl_crypto::agreement::ecdh_p384::{EcdhP384, EcdhP384PrivateKey};

use crate::{cavp_cdh_fixtures::CASES, support};

/// RFC 5903 §8.2 initiator private key `i`.
const INITIATOR_PRIVATE: &str = "099f3c7034d4a2c699884d73a375a67f7624ef7c6b3c0f160647b67414dce655e35b538041e649ee3faef896783ab194";
/// RFC 5903 §8.2 initiator public point `(gix, giy)`.
const INITIATOR_X: &str = "667842d7d180ac2cde6f74f37551f55755c7645c20ef73e31634fe72b4c55ee6de3ac808acb4bdb4c88732aee95f41aa";
const INITIATOR_Y: &str = "9482ed1fc0eeb9cafc4984625ccfc23f65032149e0e144ada024181535a0f38eeb9fcff3c2c947dae69b4c634573a81c";
/// RFC 5903 §8.2 responder private key `r`.
const RESPONDER_PRIVATE: &str = "41cb0779b4bdb85d47846725fbec3c9430fab46cc8dc5060855cc9bda0aa2942e0308312916b8ed2960e4bd55a7448fc";
/// RFC 5903 §8.2 responder public point `(grx, gry)`.
const RESPONDER_X: &str = "e558dbef53eecde3d3fccfc1aea08a89a987475d12fd950d83cfa41732bc509d0d1ac43a0336def96fda41d0774a3571";
const RESPONDER_Y: &str = "dcfbec7aacf3196472169e838430367f66eebe3c6e70c416dd5f0c68759dd1fff83fa40142209dff5eaad96db9e6386c";
/// RFC 5903 §8.2 common value `girx`, which the RFC names as the shared secret.
const SHARED_X: &str = "11187331c279962d93d604243fd592cb9d0a926f422e47187521287e7156c5c4d603135569b9e9d09cf5d4a270f59746";

/// Published evidence: both RFC 5903 §8.2 public points derive from their private keys.
#[test]
fn rfc_5903_public_points_derive_from_the_published_private_keys() {
    let initiator = EcdhP384PrivateKey::from_bytes(support::decode(INITIATOR_PRIVATE)).unwrap();
    let responder = EcdhP384PrivateKey::from_bytes(support::decode(RESPONDER_PRIVATE)).unwrap();

    assert_eq!(
        EcdhP384::public_key(&initiator).as_bytes(),
        &support::uncompressed(INITIATOR_X, INITIATOR_Y)
    );
    assert_eq!(
        EcdhP384::public_key(&responder).as_bytes(),
        &support::uncompressed(RESPONDER_X, RESPONDER_Y)
    );
}

/// Published evidence: the complete RFC 5903 §8.2 exchange reaches `girx` from both sides.
#[test]
fn rfc_5903_exchange_reaches_the_published_shared_secret_from_both_peers() {
    let initiator = EcdhP384PrivateKey::from_bytes(support::decode(INITIATOR_PRIVATE)).unwrap();
    let responder = EcdhP384PrivateKey::from_bytes(support::decode(RESPONDER_PRIVATE)).unwrap();
    let expected: [u8; 48] = support::decode(SHARED_X);

    let from_initiator =
        EcdhP384::agree(&initiator, &support::public_key(RESPONDER_X, RESPONDER_Y)).unwrap();
    let from_responder =
        EcdhP384::agree(&responder, &support::public_key(INITIATOR_X, INITIATOR_Y)).unwrap();

    assert_eq!(from_initiator.expose_secret(), &expected);
    assert_eq!(from_responder.expose_secret(), &expected);
}

/// Published evidence: all 25 CAVP ECC CDH P-384 cases derive `QIUT` and reach `ZIUT`.
#[test]
fn cavp_ecc_cdh_primitive_cases_derive_public_points_and_shared_secrets() {
    for case in &CASES {
        let private = EcdhP384PrivateKey::from_bytes(support::decode(case.private))
            .unwrap_or_else(|_| panic!("COUNT {} private key is in range", case.count));
        let expected_public = support::uncompressed(case.public_x, case.public_y);
        let expected_shared: [u8; 48] = support::decode(case.shared);

        assert_eq!(
            EcdhP384::public_key(&private).as_bytes(),
            &expected_public,
            "COUNT {} public point",
            case.count
        );
        let shared = EcdhP384::agree(&private, &support::public_key(case.peer_x, case.peer_y))
            .unwrap_or_else(|_| panic!("COUNT {} agreement succeeds", case.count));
        assert_eq!(
            shared.expose_secret(),
            &expected_shared,
            "COUNT {} shared secret",
            case.count
        );
    }
}
