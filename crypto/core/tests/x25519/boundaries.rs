//! Encoding, rejection, ownership, and generic-contract evidence.

use rsl_crypto::{
    CryptoError,
    agreement::{
        KeyAgreement,
        x25519::{X25519, X25519PrivateKey, X25519PublicKey},
    },
};

/// Standard-derived evidence that RFC 7748's unused high coordinate bit is ignored.
#[test]
fn setting_the_unused_coordinate_bit_does_not_change_the_result() {
    let private_key = X25519PrivateKey::new([0x42; 32]);
    let mut ordinary_bytes = [0x24; 32];
    ordinary_bytes[31] &= 0x7f;
    let mut high_bit_bytes = ordinary_bytes;
    high_bit_bytes[31] |= 0x80;
    let ordinary = X25519PublicKey::new(ordinary_bytes);
    let high_bit = X25519PublicKey::new(high_bit_bytes);

    let ordinary_shared = X25519::agree(&private_key, &ordinary).expect("result is nonzero");
    let high_bit_shared = X25519::agree(&private_key, &high_bit).expect("result is nonzero");

    assert_eq!(
        ordinary_shared.expose_secret(),
        high_bit_shared.expose_secret()
    );
}

/// Standard-derived evidence that `p + 9` is accepted and processed as base coordinate nine.
#[test]
fn noncanonical_modulus_plus_nine_matches_the_base_coordinate() {
    let private_key = X25519PrivateKey::new([0x42; 32]);
    let mut base_bytes = [0_u8; 32];
    base_bytes[0] = 9;
    let mut modulus_plus_nine = [0xff_u8; 32];
    modulus_plus_nine[0] = 0xf6;
    modulus_plus_nine[31] = 0x7f;
    let base = X25519PublicKey::new(base_bytes);
    let noncanonical = X25519PublicKey::new(modulus_plus_nine);

    let base_shared = X25519::agree(&private_key, &base).expect("basepoint result is nonzero");
    let noncanonical_shared =
        X25519::agree(&private_key, &noncanonical).expect("equivalent result is nonzero");

    assert_eq!(
        base_shared.expose_secret(),
        noncanonical_shared.expose_secret()
    );
}

/// RFC 7748 §6.1-derived rejection evidence for zero and a noncanonical encoding of zero.
#[test]
fn zero_one_and_their_noncanonical_equivalents_are_rejected_as_small_order_inputs() {
    let private_key = X25519PrivateKey::new([0x42; 32]);
    let zero = X25519PublicKey::new([0; 32]);
    let mut one_bytes = [0_u8; 32];
    one_bytes[0] = 1;
    let one = X25519PublicKey::new(one_bytes);
    let mut modulus_bytes = [0xff_u8; 32];
    modulus_bytes[0] = 0xed;
    modulus_bytes[31] = 0x7f;
    let modulus = X25519PublicKey::new(modulus_bytes);
    let mut modulus_plus_one_bytes = modulus_bytes;
    modulus_plus_one_bytes[0] = 0xee;
    let modulus_plus_one = X25519PublicKey::new(modulus_plus_one_bytes);

    assert!(matches!(
        X25519::agree(&private_key, &zero),
        Err(CryptoError::InvalidPublicKey)
    ));
    assert!(matches!(
        X25519::agree(&private_key, &one),
        Err(CryptoError::InvalidPublicKey)
    ));
    assert!(matches!(
        X25519::agree(&private_key, &modulus),
        Err(CryptoError::InvalidPublicKey)
    ));
    assert!(matches!(
        X25519::agree(&private_key, &modulus_plus_one),
        Err(CryptoError::InvalidPublicKey)
    ));
}

/// API-regression evidence that the generic contract reaches the same X25519 path.
#[test]
fn generic_key_agreement_contract_derives_the_same_secret() {
    fn public_key<A: KeyAgreement>(private_key: &A::PrivateKey) -> A::PublicKey {
        A::public_key(private_key)
    }

    fn agree<A: KeyAgreement>(
        private_key: &A::PrivateKey,
        peer_public_key: &A::PublicKey,
    ) -> rsl_crypto::Result<A::SharedSecret> {
        A::agree(private_key, peer_public_key)
    }

    let alice = X25519PrivateKey::new([0x11; 32]);
    let bob = X25519PrivateKey::new([0x22; 32]);
    let alice_public = public_key::<X25519>(&alice);
    let bob_public = public_key::<X25519>(&bob);
    let alice_shared = agree::<X25519>(&alice, &bob_public).expect("valid agreement");
    let bob_shared = agree::<X25519>(&bob, &alice_public).expect("valid agreement");

    assert_eq!(alice_shared.expose_secret(), bob_shared.expose_secret());
}

/// Public decoder evidence that only the fixed wire length is rejected at parse time.
#[test]
fn public_key_decoder_enforces_only_exact_length() {
    assert!(matches!(
        X25519PublicKey::try_from([0_u8; 31].as_slice()),
        Err(CryptoError::InvalidLength {
            name: "X25519 public key",
            expected: 32,
            actual: 31,
        })
    ));
    assert!(X25519PublicKey::try_from([0xff_u8; 32].as_slice()).is_ok());
    assert!(matches!(
        X25519PublicKey::try_from([0_u8; 33].as_slice()),
        Err(CryptoError::InvalidLength {
            name: "X25519 public key",
            expected: 32,
            actual: 33,
        })
    ));
}
