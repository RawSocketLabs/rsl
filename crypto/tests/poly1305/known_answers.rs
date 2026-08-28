//! Published Poly1305 evidence from RFC 8439 §2.5.2 and Appendix A.3.

use rsl_crypto::{
    CryptoError,
    mac::{
        Mac,
        poly1305::{Poly1305, Poly1305Key, Poly1305Tag},
    },
};

use crate::{rfc_fixtures::CASES, support};

/// Published evidence: all eleven A.3 vectors, including the `r = 0` weak key (vector 2) and
/// the seven reduction edge cases.
#[test]
fn appendix_a3_vectors() {
    for (index, case) in CASES.iter().enumerate() {
        let key = Poly1305Key::new(support::decode_array(case.key));
        let text = support::decode(case.text);
        let tag = Poly1305::authenticate(key, &text);
        assert_eq!(
            tag.as_bytes().as_slice(),
            support::decode(case.tag).as_slice(),
            "A.3 vector {}: {}",
            index + 1,
            case.comment
        );
        let mut verifier = Poly1305::new(Poly1305Key::new(support::decode_array(case.key)));
        verifier.update(&text);
        verifier.verify(support::decode(case.tag)).unwrap();
    }
}

/// Published evidence: the §2.5.2 tag through the generic `Mac` contract and exact tag length.
#[test]
fn section_2_5_2_through_the_generic_contract() {
    let key = support::decode("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b");
    let mut mac = <Poly1305 as Mac>::new(&key).unwrap();
    Mac::update(&mut mac, b"Cryptographic Forum Research Group").unwrap();
    let tag = Mac::finalize(mac);
    assert_eq!(
        tag,
        Poly1305Tag::new(support::decode_array("a8061dc1305136c6c22b8baf0c0127a9"))
    );
    assert_eq!(
        <Poly1305 as Mac>::new(&key[..31]).err(),
        Some(CryptoError::InvalidLength {
            name: "Poly1305 key",
            expected: 32,
            actual: 31,
        })
    );
    assert_eq!(
        Poly1305Tag::try_from(&tag.as_bytes()[..15]).err(),
        Some(CryptoError::InvalidLength {
            name: "Poly1305 tag",
            expected: 16,
            actual: 15,
        })
    );
}
