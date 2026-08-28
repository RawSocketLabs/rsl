//! Differential comparison with development-only `RustCrypto` `aes-gcm` 0.11.1.

use aes_gcm::{AeadInOut, Aes256Gcm as OracleAes256Gcm, KeyInit};
use rsl_crypto::aead::gcm::{Aes256Gcm, Aes256GcmKey, Aes256GcmNonce, Aes256GcmTag};

const LENGTHS: [usize; 14] = [0, 1, 7, 15, 16, 17, 31, 32, 33, 47, 63, 64, 65, 129];

fn fixed_bytes<const N: usize>(case: usize, domain: u8) -> [u8; N] {
    core::array::from_fn(|index| {
        let case_byte = u8::try_from(case).unwrap();
        let index_byte = u8::try_from(index).unwrap();
        case_byte
            .wrapping_mul(0x3d)
            .wrapping_add(index_byte.wrapping_mul(0x71))
            .wrapping_add(domain)
    })
}

fn variable_bytes(length: usize, case: usize, domain: u8) -> Vec<u8> {
    (0..length)
        .map(|index| {
            let case_byte = u8::try_from(case).unwrap();
            let index_byte = u8::try_from(index % 256).unwrap();
            case_byte
                .wrapping_mul(0x53)
                .wrapping_add(index_byte.wrapping_mul(0x97))
                .wrapping_add(domain)
        })
        .collect()
}

#[test]
fn varied_keys_nonces_aad_and_payloads_match_rustcrypto() {
    for case in 0..32 {
        let key = fixed_bytes::<32>(case, 0x11);
        let nonce = fixed_bytes::<12>(case, 0x22);
        let plaintext = variable_bytes(LENGTHS[case % LENGTHS.len()], case, 0x33);
        let aad = variable_bytes(LENGTHS[(case * 5 + 3) % LENGTHS.len()], case, 0x44);

        let mut reference_buffer = plaintext.clone();
        let reference = OracleAes256Gcm::new(&key.into());
        let reference_tag = reference
            .encrypt_inout_detached(&nonce.into(), &aad, reference_buffer.as_mut_slice().into())
            .unwrap();

        let ours = Aes256Gcm::new(Aes256GcmKey::new(key));
        let our_nonce = Aes256GcmNonce::new(nonce);
        let sealed = ours.seal(&our_nonce, &aad, &plaintext).unwrap();
        assert_eq!(
            sealed.ciphertext(),
            reference_buffer.as_slice(),
            "case {case} ciphertext"
        );
        assert_eq!(
            sealed.tag().as_bytes().as_slice(),
            reference_tag.as_slice(),
            "case {case} tag"
        );

        let opened = ours
            .open(
                &our_nonce,
                &aad,
                &reference_buffer,
                &Aes256GcmTag::try_from(reference_tag.as_slice()).unwrap(),
            )
            .unwrap();
        assert_eq!(opened, plaintext, "case {case} open");
    }
}
