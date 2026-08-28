//! Differential signing and strict-verification evidence against ed25519-dalek 3.0.0.

use ed25519_dalek::{
    Signature as ReferenceSignature, Signer as _, SigningKey as ReferenceSigningKey,
};
use rsl_crypto::signature::ed25519::{Ed25519Signature, Ed25519SigningKey, Ed25519VerifyingKey};

#[test]
fn deterministic_keys_messages_and_signatures_match_dalek() {
    for case in 0_u8..32 {
        let seed = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every seed byte index fits u8");
            case.wrapping_mul(0x53)
                .wrapping_add(index.wrapping_mul(0x1d))
        });
        let message: Vec<u8> = (0..usize::from(case) * 7 + 3)
            .map(|index| {
                let index = u8::try_from(index).expect("every test message index fits u8");
                case.wrapping_add(index.wrapping_mul(0x71))
            })
            .collect();

        let ours = Ed25519SigningKey::from_seed(seed);
        let reference = ReferenceSigningKey::from_bytes(&seed);
        assert_eq!(
            ours.verifying_key().as_bytes(),
            reference.verifying_key().as_bytes()
        );

        let our_signature = ours.sign(&message).unwrap();
        let reference_signature: ReferenceSignature = reference.sign(&message);
        assert_eq!(our_signature.as_bytes(), &reference_signature.to_bytes());

        reference
            .verifying_key()
            .verify_strict(
                &message,
                &ReferenceSignature::from_bytes(our_signature.as_bytes()),
            )
            .unwrap();
        Ed25519VerifyingKey::from_bytes(*reference.verifying_key().as_bytes())
            .unwrap()
            .verify(
                &message,
                &Ed25519Signature::from_bytes(reference_signature.to_bytes()),
            )
            .unwrap();
    }
}
