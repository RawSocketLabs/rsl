//! Differential `AEAD_CHACHA20_POLY1305` evidence against the `chacha20poly1305` crate 0.11.0.

use chacha20poly1305::{
    ChaCha20Poly1305 as Reference, KeyInit,
    aead::{Aead as _, Payload},
};
use rsl_crypto::aead::chacha20poly1305::{
    ChaCha20Poly1305, ChaCha20Poly1305Key, ChaCha20Poly1305Nonce, ChaCha20Poly1305Tag,
};

/// Differential evidence over 32 varied keys, nonces, AAD lengths, and payload lengths.
#[test]
fn seal_and_open_match_the_reference_in_both_directions() {
    for case in 0_u8..32 {
        let key: [u8; 32] = core::array::from_fn(|index| {
            let index = u8::try_from(index).unwrap();
            case.wrapping_mul(0x53)
                .wrapping_add(index.wrapping_mul(0x1d))
        });
        let nonce: [u8; 12] = core::array::from_fn(|index| {
            let index = u8::try_from(index).unwrap();
            case.wrapping_mul(0x71).wrapping_add(index)
        });
        let aad: Vec<u8> = (0..usize::from(case) % 21)
            .map(|i| u8::try_from(i).unwrap() ^ case)
            .collect();
        let plaintext: Vec<u8> = (0..usize::from(case) * 13)
            .map(|i| u8::try_from(i % 251).unwrap().wrapping_add(case))
            .collect();

        let reference = Reference::new((&key).into());
        let expected = reference
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .unwrap();
        let (expected_ciphertext, expected_tag) = expected.split_at(plaintext.len());

        let ours = ChaCha20Poly1305::new(ChaCha20Poly1305Key::new(key));
        let our_nonce = ChaCha20Poly1305Nonce::new(nonce);
        let sealed = ours.seal(&our_nonce, &aad, &plaintext).unwrap();
        assert_eq!(
            sealed.ciphertext(),
            expected_ciphertext,
            "case {case} ciphertext"
        );
        assert_eq!(
            sealed.tag().as_bytes().as_slice(),
            expected_tag,
            "case {case} tag"
        );

        let opened = ours
            .open(
                &our_nonce,
                &aad,
                expected_ciphertext,
                &ChaCha20Poly1305Tag::try_from(expected_tag).unwrap(),
            )
            .unwrap();
        assert_eq!(opened, plaintext, "case {case} open");
        let mut combined = sealed.ciphertext().to_vec();
        combined.extend_from_slice(sealed.tag().as_bytes());
        assert_eq!(
            reference
                .decrypt(
                    (&nonce).into(),
                    Payload {
                        msg: &combined,
                        aad: &aad
                    }
                )
                .unwrap(),
            plaintext,
            "case {case} reference opens ours"
        );
    }
}
