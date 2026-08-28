//! Published ECDH P-256 evidence from RFC 5903 §8.1 and NIST CAVP ECC CDH primitive cases.

use rsl_crypto::agreement::ecdh_p256::{EcdhP256, EcdhP256PrivateKey};

use crate::{cavp_cdh_fixtures::CASES, support};

/// RFC 5903 §8.1 initiator private key `i`.
const INITIATOR_PRIVATE: &str = "C88F01F510D9AC3F70A292DAA2316DE544E9AAB8AFE84049C62A9C57862D1433";
/// RFC 5903 §8.1 initiator public point `(gix, giy)`.
const INITIATOR_X: &str = "DAD0B65394221CF9B051E1FECA5787D098DFE637FC90B9EF945D0C3772581180";
const INITIATOR_Y: &str = "5271A0461CDB8252D61F1C456FA3E59AB1F45B33ACCF5F58389E0577B8990BB3";
/// RFC 5903 §8.1 responder private key `r`.
const RESPONDER_PRIVATE: &str = "C6EF9C5D78AE012A011164ACB397CE2088685D8F06BF9BE0B283AB46476BEE53";
/// RFC 5903 §8.1 responder public point `(grx, gry)`.
const RESPONDER_X: &str = "D12DFB5289C8D4F81208B70270398C342296970A0BCCB74C736FC7554494BF63";
const RESPONDER_Y: &str = "56FBF3CA366CC23E8157854C13C58D6AAC23F046ADA30F8353E74F33039872AB";
/// RFC 5903 §8.1 common value `girx`, which the RFC names as the shared secret.
const SHARED_X: &str = "D6840F6B42F6EDAFD13116E0E12565202FEF8E9ECE7DCE03812464D04B9442DE";

/// Published evidence: both RFC 5903 §8.1 public points derive from their private keys.
#[test]
fn rfc_5903_public_points_derive_from_the_published_private_keys() {
    let initiator = EcdhP256PrivateKey::from_bytes(support::decode(INITIATOR_PRIVATE)).unwrap();
    let responder = EcdhP256PrivateKey::from_bytes(support::decode(RESPONDER_PRIVATE)).unwrap();

    assert_eq!(
        EcdhP256::public_key(&initiator).as_bytes(),
        &support::uncompressed(INITIATOR_X, INITIATOR_Y)
    );
    assert_eq!(
        EcdhP256::public_key(&responder).as_bytes(),
        &support::uncompressed(RESPONDER_X, RESPONDER_Y)
    );
}

/// Published evidence: the complete RFC 5903 §8.1 exchange reaches `girx` from both sides.
#[test]
fn rfc_5903_exchange_reaches_the_published_shared_secret_from_both_peers() {
    let initiator = EcdhP256PrivateKey::from_bytes(support::decode(INITIATOR_PRIVATE)).unwrap();
    let responder = EcdhP256PrivateKey::from_bytes(support::decode(RESPONDER_PRIVATE)).unwrap();
    let expected: [u8; 32] = support::decode(SHARED_X);

    let from_initiator =
        EcdhP256::agree(&initiator, &support::public_key(RESPONDER_X, RESPONDER_Y)).unwrap();
    let from_responder =
        EcdhP256::agree(&responder, &support::public_key(INITIATOR_X, INITIATOR_Y)).unwrap();

    assert_eq!(from_initiator.expose_secret(), &expected);
    assert_eq!(from_responder.expose_secret(), &expected);
}

/// Published evidence: all 25 CAVP ECC CDH P-256 cases derive `QIUT` and reach `ZIUT`.
#[test]
fn cavp_ecc_cdh_primitive_cases_derive_public_points_and_shared_secrets() {
    for case in &CASES {
        let private = EcdhP256PrivateKey::from_bytes(support::decode(case.private))
            .unwrap_or_else(|_| panic!("COUNT {} private key is in range", case.count));
        let expected_public = support::uncompressed(case.public_x, case.public_y);
        let expected_shared: [u8; 32] = support::decode(case.shared);

        assert_eq!(
            EcdhP256::public_key(&private).as_bytes(),
            &expected_public,
            "COUNT {} public point",
            case.count
        );
        let shared = EcdhP256::agree(&private, &support::public_key(case.peer_x, case.peer_y))
            .unwrap_or_else(|_| panic!("COUNT {} agreement succeeds", case.count));
        assert_eq!(
            shared.expose_secret(),
            &expected_shared,
            "COUNT {} shared secret",
            case.count
        );
    }
}
