//! Differential comparison with development-only `RustCrypto` `aes-gcm` 0.11.1.

use aes_gcm::{AeadInOut, Aes128Gcm as OracleAes128Gcm, KeyInit};
use rsl_crypto::aead::gcm::{Aes128Gcm, Aes128GcmKey, Aes128GcmNonce, Aes128GcmTag};

const LENGTHS: [usize; 14] = [0, 1, 7, 15, 16, 17, 31, 32, 33, 47, 63, 64, 65, 129];

fn fixed_bytes<const N: usize>(case: usize, domain: u8) -> [u8; N] {
    core::array::from_fn(|index| {
        let case_byte = u8::try_from(case).expect("the test has fewer than 256 cases");
        let index_byte = u8::try_from(index).expect("the fixed arrays have fewer than 256 bytes");

        case_byte
            .wrapping_mul(0x3d)
            .wrapping_add(index_byte.wrapping_mul(0x71))
            .wrapping_add(domain)
    })
}

fn variable_bytes(length: usize, case: usize, domain: u8) -> Vec<u8> {
    (0..length)
        .map(|index| {
            let case_byte = u8::try_from(case).expect("the test has fewer than 256 cases");
            let index_byte = u8::try_from(index % 256).expect("the remainder always fits in u8");

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
        let key = fixed_bytes::<16>(case, 0x11);
        let nonce = fixed_bytes::<12>(case, 0x22);
        let plaintext_len = LENGTHS[case % LENGTHS.len()];
        let associated_data_len = LENGTHS[(case * 5 + 3) % LENGTHS.len()];
        let plaintext = variable_bytes(plaintext_len, case, 0x33);
        let associated_data = variable_bytes(associated_data_len, case, 0x44);

        let ours = Aes128Gcm::new(Aes128GcmKey::new(key));
        let ours_nonce = Aes128GcmNonce::new(nonce);
        let ours_sealed = ours
            .seal(&ours_nonce, &associated_data, &plaintext)
            .expect("the differential fixture is far below GCM's limits");

        let oracle = OracleAes128Gcm::new(&key.into());
        let oracle_nonce = nonce.into();
        let mut oracle_ciphertext = plaintext.clone();
        let oracle_tag = oracle
            .encrypt_inout_detached(
                &oracle_nonce,
                &associated_data,
                oracle_ciphertext.as_mut_slice().into(),
            )
            .expect("the differential fixture is far below the oracle's limits");

        assert_eq!(ours_sealed.ciphertext(), oracle_ciphertext);
        assert_eq!(ours_sealed.tag().as_bytes(), oracle_tag.as_slice());

        let oracle_tag_bytes: [u8; 16] = oracle_tag.into();
        let opened = ours
            .open(
                &ours_nonce,
                &associated_data,
                &oracle_ciphertext,
                &Aes128GcmTag::new(oracle_tag_bytes),
            )
            .expect("RSL must accept the oracle's matching output");
        assert_eq!(opened, plaintext);
    }
}
