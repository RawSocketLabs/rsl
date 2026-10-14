//! Published RFC 7748 X25519 vectors through the public API.

use rsl_crypto::agreement::x25519::{X25519, X25519PrivateKey, X25519PublicKey};

use crate::support::decode_32;

/// Published complete-function evidence from RFC 7748 §5.2, X25519 vector one.
#[test]
fn first_direct_scalar_multiplication_vector() {
    assert_scalar_multiplication(
        "a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4",
        "e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c",
        "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552",
    );
}

/// Published complete-function and high-input-bit evidence from RFC 7748 §5.2, vector two.
#[test]
fn second_direct_scalar_multiplication_vector() {
    assert_scalar_multiplication(
        "4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d",
        "e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493",
        "95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957",
    );
}

/// Published iterative evidence from RFC 7748 §5.2 after one application.
#[test]
fn one_iteration_matches_the_published_result() {
    assert_eq!(
        iterate(1),
        decode_32("422c8e7a6227d7bca1350b3e2bb7279f7897b87bb6854b783c60e80311ae3079")
    );
}

/// Published iterative evidence from RFC 7748 §5.2 after 1,000 applications.
#[test]
fn one_thousand_iterations_match_the_published_result() {
    assert_eq!(
        iterate(1_000),
        decode_32("684cf59ba83309552800ef566f2f4d3c1c3887c49360e3875f2eb94d99532c51")
    );
}

/// Published complete Diffie-Hellman evidence from RFC 7748 §6.1.
#[test]
fn alice_and_bob_derive_the_published_public_keys_and_shared_secret() {
    let alice_private = X25519PrivateKey::new(decode_32(
        "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a",
    ));
    let alice_expected_public =
        decode_32("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
    let bob_private = X25519PrivateKey::new(decode_32(
        "5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb",
    ));
    let bob_expected_public =
        decode_32("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
    let expected_shared =
        decode_32("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");

    let alice_public = X25519::public_key(&alice_private);
    let bob_public = X25519::public_key(&bob_private);
    assert_eq!(alice_public.as_bytes(), &alice_expected_public);
    assert_eq!(bob_public.as_bytes(), &bob_expected_public);

    let alice_shared = X25519::agree(&alice_private, &bob_public)
        .expect("the published Bob coordinate produces a nonzero result");
    let bob_shared = X25519::agree(&bob_private, &alice_public)
        .expect("the published Alice coordinate produces a nonzero result");
    assert_eq!(alice_shared.expose_secret(), &expected_shared);
    assert_eq!(bob_shared.expose_secret(), &expected_shared);
}

/// Apply the public checked boundary to one published direct-function vector.
fn assert_scalar_multiplication(scalar: &str, coordinate: &str, expected: &str) {
    let private_key = X25519PrivateKey::new(decode_32(scalar));
    let public_key = X25519PublicKey::new(decode_32(coordinate));
    let shared = X25519::agree(&private_key, &public_key)
        .expect("RFC 7748's direct vectors have nonzero outputs");

    assert_eq!(shared.expose_secret(), &decode_32(expected));
}

/// Reproduce RFC 7748 §5.2's iterative `k = X25519(k, u); u = old_k` process.
/// Published iterative evidence from RFC 7748 §5.2 after 1,000,000 applications. Ignored by
/// default because the deliberately unoptimized ladder takes minutes; run with `--ignored`.
#[test]
#[ignore = "one million ladder iterations; run explicitly"]
fn one_million_iterations_match_the_published_result() {
    assert_eq!(
        iterate(1_000_000),
        decode_32("7c3911e0ab2586fd864497297e575e6f3bc601c0883c30df5f4dd2d24f665424")
    );
}

fn iterate(iterations: usize) -> [u8; 32] {
    let mut scalar = decode_32("0900000000000000000000000000000000000000000000000000000000000000");
    let mut coordinate = scalar;

    for _ in 0..iterations {
        let old_scalar = scalar;
        let private_key = X25519PrivateKey::new(scalar);
        let public_key = X25519PublicKey::new(coordinate);
        scalar = X25519::agree(&private_key, &public_key)
            .expect("the published iterative sequence has nonzero checkpoints")
            .into_inner();
        coordinate = old_scalar;
    }

    scalar
}
