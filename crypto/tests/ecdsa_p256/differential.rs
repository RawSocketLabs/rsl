//! Differential ECDSA P-256/SHA-256 evidence against the `p256` crate 0.14.0.

use p256::{
    SecretKey,
    ecdsa::{Signature, SigningKey, VerifyingKey, signature::Signer},
};
use rsl_crypto::{
    CryptoError,
    signature::ecdsa_p256::{EcdsaP256Signature, EcdsaP256VerifyingKey},
};

/// Differential evidence: 32 reference signatures over varied keys and messages verify here,
/// and tampered copies do not.
#[test]
fn reference_signatures_verify_and_tampered_copies_fail() {
    for case in 0_u8..32 {
        let private: [u8; 32] = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every scalar byte index fits in u8");
            case.wrapping_mul(0x53)
                .wrapping_add(index.wrapping_mul(0x1d))
                .wrapping_add(1)
        });
        let message: Vec<u8> = (0..usize::from(case) * 9 + 1)
            .map(|index| {
                let index = u8::try_from(index % 251).expect("reduced index fits in u8");
                case.wrapping_add(index.wrapping_mul(0x71))
            })
            .collect();

        let secret = SecretKey::from_slice(&private).unwrap();
        let signing = SigningKey::from(&secret);
        let reference_signature: Signature = signing.sign(&message);
        let encoded_key = VerifyingKey::from(&signing).to_sec1_point(false);

        let key = EcdsaP256VerifyingKey::try_from(encoded_key.as_bytes()).unwrap();
        let signature =
            EcdsaP256Signature::try_from(reference_signature.to_bytes().as_slice()).unwrap();
        key.verify_sha256(&message, &signature)
            .unwrap_or_else(|error| panic!("case {case} verifies: {error}"));

        let mut tampered = signature.into_bytes();
        tampered[usize::from(case) * 2 % 64] ^= 0x01;
        assert_eq!(
            key.verify_sha256(&message, &EcdsaP256Signature::from_bytes(tampered)),
            Err(CryptoError::InvalidSignature),
            "case {case} tampered"
        );
    }
}
