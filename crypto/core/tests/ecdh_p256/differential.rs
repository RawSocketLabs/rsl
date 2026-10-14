//! Differential ECDH P-256 evidence against the `p256` crate 0.14.0.

use p256::{PublicKey, SecretKey, ecdh::diffie_hellman, elliptic_curve::sec1::ToSec1Point};
use rsl_crypto::agreement::ecdh_p256::{EcdhP256, EcdhP256PrivateKey, EcdhP256PublicKey};

fn deterministic_scalar(case: u8, multiplier: u8, offset: u8) -> [u8; 32] {
    core::array::from_fn(|index| {
        let index = u8::try_from(index).expect("every scalar byte index fits in u8");
        case.wrapping_mul(multiplier)
            .wrapping_add(index.wrapping_mul(0x1d))
            .wrapping_add(offset)
    })
}

/// Differential evidence over 32 deterministic key pairs: public points and shared secrets.
#[test]
fn public_derivation_and_agreement_match_the_reference() {
    for case in 0_u8..32 {
        let private_bytes = deterministic_scalar(case, 0x53, 1);
        let peer_bytes = deterministic_scalar(case, 0x71, 0x24);

        let reference_private = SecretKey::from_slice(&private_bytes).unwrap();
        let reference_peer = SecretKey::from_slice(&peer_bytes).unwrap();
        let expected_public = reference_private.public_key().to_sec1_point(false);
        let peer_public = reference_peer.public_key().to_sec1_point(false);
        let expected_shared = diffie_hellman(
            reference_private.to_nonzero_scalar(),
            reference_peer.public_key().as_affine(),
        );

        let ours = EcdhP256PrivateKey::from_bytes(private_bytes).unwrap();
        assert_eq!(
            EcdhP256::public_key(&ours).as_bytes().as_slice(),
            expected_public.as_bytes(),
            "public case {case}"
        );
        let peer = EcdhP256PublicKey::try_from(peer_public.as_bytes()).unwrap();
        assert_eq!(
            EcdhP256::agree(&ours, &peer)
                .unwrap()
                .expose_secret()
                .as_slice(),
            expected_shared.raw_secret_bytes().as_slice(),
            "shared case {case}"
        );
    }
}

/// Differential evidence: the reference accepts every point this implementation derives.
#[test]
fn derived_points_are_accepted_by_the_reference_parser() {
    for case in 0_u8..16 {
        let ours = EcdhP256PrivateKey::from_bytes(deterministic_scalar(case, 0x3d, 0x5a)).unwrap();
        let public = EcdhP256::public_key(&ours);
        PublicKey::from_sec1_bytes(public.as_bytes())
            .unwrap_or_else(|_| panic!("case {case} derived point parses"));
    }
}
