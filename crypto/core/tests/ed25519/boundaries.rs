//! Malformed-input, strictness, trait, and deterministic-signing evidence.

use rsl_crypto::{
    CryptoError, Result,
    random::RandomSource,
    signature::{Signer, Verifier, ed25519::*},
};

#[test]
fn exact_wire_lengths_are_enforced() {
    for length in [0, 1, 31, 33, 63, 65] {
        let bytes = vec![0_u8; length];
        let expected = if length == 32 { 32 } else { 64 };
        if length != 32 {
            assert_eq!(
                Ed25519VerifyingKey::try_from(bytes.as_slice()),
                Err(CryptoError::InvalidLength {
                    name: "Ed25519 public key",
                    expected: 32,
                    actual: length,
                })
            );
        }
        if length != 64 {
            assert_eq!(
                Ed25519Signature::try_from(bytes.as_slice()),
                Err(CryptoError::InvalidLength {
                    name: "Ed25519 signature",
                    expected,
                    actual: length,
                })
            );
        }
    }
}

#[test]
fn changed_message_and_every_signature_region_fail_uniformly() {
    let signing = Ed25519SigningKey::from_seed([0x42; 32]);
    let verifying = signing.verifying_key();
    let signature = signing.sign(b"original").unwrap();
    assert_eq!(
        verifying.verify(b"changed", &signature),
        Err(CryptoError::InvalidSignature)
    );

    for index in [0, 1, 15, 31, 32, 47, 63] {
        let mut changed = signature.into_bytes();
        changed[index] ^= 1;
        assert_eq!(
            verifying.verify(b"original", &Ed25519Signature::from_bytes(changed)),
            Err(CryptoError::InvalidSignature),
            "changed signature byte {index}"
        );
    }
}

#[test]
fn noncanonical_s_is_rejected() {
    let signing = Ed25519SigningKey::from_seed([0x24; 32]);
    let verifying = signing.verifying_key();
    let mut bytes = signing.sign(b"message").unwrap().into_bytes();
    bytes[32..].copy_from_slice(&[
        0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde,
        0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
    ]);
    assert_eq!(
        verifying.verify(b"message", &Ed25519Signature::from_bytes(bytes)),
        Err(CryptoError::InvalidSignature)
    );
}

#[test]
fn signing_is_deterministic_and_generic_contracts_dispatch() {
    struct IgnoredRandom;
    impl RandomSource for IgnoredRandom {
        fn fill_bytes(&mut self, _: &mut [u8]) -> Result<()> {
            panic!("pure Ed25519 signing must not request runtime randomness")
        }
    }

    let signing = Ed25519SigningKey::from_seed([0x7a; 32]);
    let verifying = signing.verifying_key();
    let inherent = signing.sign(b"same bytes").unwrap();
    let through_trait = Signer::sign(&signing, &mut IgnoredRandom, b"same bytes").unwrap();
    assert_eq!(inherent, through_trait);
    Verifier::verify(&verifying, b"same bytes", &through_trait).unwrap();
}

#[test]
fn caller_entropy_source_generates_the_seed() {
    struct Fixed(u8);
    impl RandomSource for Fixed {
        fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
            for byte in output {
                *byte = self.0;
                self.0 = self.0.wrapping_add(1);
            }
            Ok(())
        }
    }
    let generated = Ed25519SigningKey::generate(&mut Fixed(0)).unwrap();
    let expected = Ed25519SigningKey::from_seed(core::array::from_fn(|index| {
        u8::try_from(index).expect("every seed byte index fits u8")
    }));
    assert_eq!(generated.verifying_key(), expected.verifying_key());
}

/// RFC 8032 §5.1-derived evidence: context length limits and variant separation.
#[test]
fn context_length_limits_and_variant_separation_are_enforced() {
    use rsl_crypto::digest::sha2::sha512::Sha512;

    assert_eq!(
        Ed25519Context::new(b"").err(),
        Some(CryptoError::InvalidLength {
            name: "Ed25519 context",
            expected: 1,
            actual: 0,
        })
    );
    assert_eq!(
        Ed25519Context::new(&[0x61; 256]).err(),
        Some(CryptoError::InvalidLength {
            name: "Ed25519 context",
            expected: 255,
            actual: 256,
        })
    );
    let longest = Ed25519Context::new(&[0x61; 255]).unwrap();
    assert_eq!(longest.as_bytes().len(), 255);

    let signing = Ed25519SigningKey::from_seed([0x24; 32]);
    let verifying = signing.verifying_key();
    let foo = Ed25519Context::new(b"foo").unwrap();
    let bar = Ed25519Context::new(b"bar").unwrap();
    let message = b"same message";
    let digest = Sha512::digest(message).unwrap();

    let pure = signing.sign(message).unwrap();
    let with_foo = signing.sign_with_context(&foo, message).unwrap();
    let prehashed = signing.sign_prehashed(&digest, None).unwrap();
    let prehashed_foo = signing.sign_prehashed(&digest, Some(&foo)).unwrap();

    assert!(
        verifying
            .verify_with_context(&foo, message, &with_foo)
            .is_ok()
    );
    assert!(
        verifying
            .verify_with_context(&bar, message, &with_foo)
            .is_err()
    );
    assert!(verifying.verify(message, &with_foo).is_err());
    assert!(
        verifying
            .verify_prehashed(&digest, None, &prehashed)
            .is_ok()
    );
    assert!(
        verifying
            .verify_prehashed(&digest, Some(&foo), &prehashed)
            .is_err()
    );
    assert!(
        verifying
            .verify_prehashed(&digest, Some(&foo), &prehashed_foo)
            .is_ok()
    );
    assert!(verifying.verify_prehashed(&digest, None, &pure).is_err());
    assert!(verifying.verify(message, &prehashed).is_err());
    assert!(
        verifying
            .verify_with_context(&longest, message, &with_foo)
            .is_err()
    );
}
