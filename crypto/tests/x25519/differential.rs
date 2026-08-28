//! Differential X25519 evidence against `x25519-dalek` 3.0.0.

use rsl_crypto::agreement::x25519::{X25519, X25519PrivateKey, X25519PublicKey};
use x25519_dalek::{X25519_BASEPOINT_BYTES, x25519 as reference_x25519};

/// Differential evidence over deterministic private keys and peer public coordinates.
#[test]
fn public_derivation_and_agreement_match_rustcrypto() {
    for case in 0_u8..64 {
        let private_bytes = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every X25519 byte index fits in u8");
            case.wrapping_mul(0x53)
                .wrapping_add(index.wrapping_mul(0x1d))
                .wrapping_add(1)
        });
        let peer_private_bytes = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every X25519 byte index fits in u8");
            case.wrapping_mul(0x71)
                .wrapping_add(index.wrapping_mul(0x2f))
                .wrapping_add(0x24)
        });
        let expected_public = reference_x25519(private_bytes, X25519_BASEPOINT_BYTES);
        let peer_public_bytes = reference_x25519(peer_private_bytes, X25519_BASEPOINT_BYTES);
        let expected_shared = reference_x25519(private_bytes, peer_public_bytes);

        let private_key = X25519PrivateKey::new(private_bytes);
        let our_public = X25519::public_key(&private_key);
        let peer_public = X25519PublicKey::new(peer_public_bytes);
        let our_shared = X25519::agree(&private_key, &peer_public)
            .expect("generated peer public keys produce contributory results");

        assert_eq!(
            our_public.as_bytes(),
            &expected_public,
            "public case {case}"
        );
        assert_eq!(
            our_shared.expose_secret(),
            &expected_shared,
            "shared case {case}"
        );
    }
}

/// Differential evidence for arbitrary encoded coordinates, including high-bit masking.
#[test]
fn arbitrary_coordinate_encodings_match_rustcrypto() {
    for case in 0_u8..64 {
        let private_bytes = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every X25519 byte index fits in u8");
            case.wrapping_mul(0x3d)
                .wrapping_add(index.wrapping_mul(0x17))
                .wrapping_add(0x5a)
        });
        let coordinate_bytes = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every X25519 byte index fits in u8");
            case.wrapping_mul(0x29)
                .wrapping_add(index.wrapping_mul(0x43))
                .wrapping_add(1)
        });
        let expected = reference_x25519(private_bytes, coordinate_bytes);
        let private_key = X25519PrivateKey::new(private_bytes);
        let coordinate = X25519PublicKey::new(coordinate_bytes);

        match X25519::agree(&private_key, &coordinate) {
            Ok(shared) => assert_eq!(shared.expose_secret(), &expected, "case {case}"),
            Err(error) => {
                assert_eq!(error, rsl_crypto::CryptoError::InvalidPublicKey);
                assert_eq!(expected, [0_u8; 32], "only all-zero output is rejected");
            }
        }
    }
}
