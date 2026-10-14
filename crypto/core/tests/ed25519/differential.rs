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

/// Differential Ed25519ph evidence against ed25519-dalek's prehashed path, with and without a
/// context.
#[test]
fn prehashed_signatures_match_dalek_in_both_directions() {
    use ed25519_dalek::Sha512 as ReferenceSha512;
    use rsl_crypto::{digest::sha2::sha512::Sha512, signature::ed25519::Ed25519Context};
    use sha2::Digest as _;

    for case in 0_u8..16 {
        let seed: [u8; 32] = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every seed byte index fits u8");
            case.wrapping_mul(0x2f)
                .wrapping_add(index.wrapping_mul(0x31))
        });
        let message: Vec<u8> = (0..usize::from(case) * 11 + 5)
            .map(|index| {
                let index = u8::try_from(index % 251).expect("reduced index fits u8");
                case.wrapping_add(index.wrapping_mul(0x5b))
            })
            .collect();
        let context_bytes: Vec<u8> = (0..=usize::from(case % 5))
            .map(|index| 0x61 + u8::try_from(index).unwrap())
            .collect();
        let context = Ed25519Context::new(&context_bytes).unwrap();

        let ours = Ed25519SigningKey::from_seed(seed);
        let reference = ReferenceSigningKey::from_bytes(&seed);
        let digest = Sha512::digest(&message).unwrap();

        for context_option in [None, Some(&context)] {
            let reference_context = context_option.map(Ed25519Context::as_bytes);
            let mut reference_digest = ReferenceSha512::new();
            reference_digest.update(&message);
            let reference_signature = reference
                .sign_prehashed(reference_digest.clone(), reference_context)
                .unwrap();
            let our_signature = ours.sign_prehashed(&digest, context_option).unwrap();
            assert_eq!(
                our_signature.as_bytes(),
                &reference_signature.to_bytes(),
                "case {case} context {context_option:?}"
            );
            reference
                .verifying_key()
                .verify_prehashed(reference_digest, reference_context, &reference_signature)
                .unwrap();
            Ed25519VerifyingKey::from_bytes(*reference.verifying_key().as_bytes())
                .unwrap()
                .verify_prehashed(
                    &digest,
                    context_option,
                    &Ed25519Signature::from_bytes(reference_signature.to_bytes()),
                )
                .unwrap();
        }
    }
}
