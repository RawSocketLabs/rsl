//! Differential ECDSA P-384/SHA-384 evidence against the `p256` crate 0.14.0.

use p384::{
    SecretKey,
    ecdsa::{Signature, SigningKey, VerifyingKey, signature::Signer},
};
use rsl_crypto::{
    CryptoError,
    signature::ecdsa_p384::{EcdsaP384Signature, EcdsaP384VerifyingKey},
};

/// Differential evidence: 32 reference signatures over varied keys and messages verify here,
/// and tampered copies do not.
#[test]
fn reference_signatures_verify_and_tampered_copies_fail() {
    for case in 0_u8..32 {
        let private: [u8; 48] = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every scalar byte index fits in u8");
            case.wrapping_mul(0x53)
                .wrapping_add(index.wrapping_mul(0x1d))
                .wrapping_add(1)
        });
        let message: Vec<u8> = (0..=usize::from(case) * 9)
            .map(|index| {
                let index = u8::try_from(index % 251).expect("reduced index fits in u8");
                case.wrapping_add(index.wrapping_mul(0x71))
            })
            .collect();

        let secret = SecretKey::from_slice(&private).unwrap();
        let signing = SigningKey::from(&secret);
        let reference_signature: Signature = signing.sign(&message);
        let encoded_key = VerifyingKey::from(&signing).to_sec1_point(false);

        let key = EcdsaP384VerifyingKey::try_from(encoded_key.as_bytes()).unwrap();
        let signature =
            EcdsaP384Signature::try_from(reference_signature.to_bytes().as_slice()).unwrap();
        key.verify_sha384(&message, &signature)
            .unwrap_or_else(|error| panic!("case {case} verifies: {error}"));

        let mut tampered = signature.into_bytes();
        tampered[usize::from(case) * 2 % 96] ^= 0x01;
        assert_eq!(
            key.verify_sha384(&message, &EcdsaP384Signature::from_bytes(tampered)),
            Err(CryptoError::InvalidSignature),
            "case {case} tampered"
        );
    }
}

/// Differential evidence: RFC 6979 deterministic signatures are byte-identical to the reference
/// over 32 keys and messages, and the reference verifies them.
#[test]
fn deterministic_signatures_match_the_reference_byte_for_byte() {
    use p384::ecdsa::signature::Verifier as _;
    use rsl_crypto::signature::ecdsa_p384::EcdsaP384SigningKey;

    for case in 0_u8..32 {
        let private: [u8; 48] = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every scalar byte index fits in u8");
            case.wrapping_mul(0x3d)
                .wrapping_add(index.wrapping_mul(0x17))
                .wrapping_add(0x5a)
        });
        let message: Vec<u8> = (0..usize::from(case) * 5 + 2)
            .map(|index| {
                let index = u8::try_from(index % 251).expect("reduced index fits in u8");
                case.wrapping_mul(3).wrapping_add(index.wrapping_mul(0x29))
            })
            .collect();

        let reference = SigningKey::from(&SecretKey::from_slice(&private).unwrap());
        let reference_signature: Signature = reference.sign(&message);
        let ours = EcdsaP384SigningKey::from_bytes(private).unwrap();
        let our_signature = ours.sign_sha384(&message).unwrap();

        assert_eq!(
            our_signature.as_bytes().as_slice(),
            reference_signature.to_bytes().as_slice(),
            "case {case}"
        );
        assert_eq!(
            ours.verifying_key().as_bytes().as_slice(),
            VerifyingKey::from(&reference)
                .to_sec1_point(false)
                .as_bytes(),
            "case {case} public point"
        );
        VerifyingKey::from(&reference)
            .verify(
                &message,
                &Signature::from_slice(our_signature.as_bytes()).unwrap(),
            )
            .unwrap_or_else(|_| panic!("case {case} accepted by the reference"));
    }
}
