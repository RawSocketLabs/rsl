//! Differential AES-128 evidence against `RustCrypto` `aes` 0.9.2.

use aes::{
    Aes128 as ReferenceAes128,
    cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit},
};
use rsl_crypto::cipher::aes::aes128::{Aes128, Aes128Block, Aes128Key};

/// Differential evidence across deterministic key and block variation.
#[test]
fn encryption_and_decryption_match_rustcrypto() {
    for key_case in 0_u8..24 {
        let key_bytes = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every AES key index fits in u8");
            key_case
                .wrapping_mul(0x43)
                .wrapping_add(index.wrapping_mul(0x1d))
        });
        let ours = Aes128::new(Aes128Key::new(key_bytes));
        let reference = ReferenceAes128::new(&Array::from(key_bytes));

        for block_case in 0_u8..8 {
            let plaintext = core::array::from_fn(|index| {
                let index = u8::try_from(index).expect("every AES block index fits in u8");
                key_case
                    .wrapping_mul(0x71)
                    .wrapping_add(block_case.wrapping_mul(0x2f))
                    .wrapping_add(index.wrapping_mul(0x13))
            });
            let mut our_block = Aes128Block::new(plaintext);
            let mut reference_block = Array::from(plaintext);

            ours.encrypt_block(&mut our_block);
            reference.encrypt_block(&mut reference_block);

            assert_eq!(
                our_block.as_bytes().as_slice(),
                reference_block.as_slice(),
                "encryption key {key_case}, block {block_case}"
            );

            ours.decrypt_block(&mut our_block);
            reference.decrypt_block(&mut reference_block);

            assert_eq!(
                our_block.as_bytes().as_slice(),
                reference_block.as_slice(),
                "decryption key {key_case}, block {block_case}"
            );
            assert_eq!(our_block.as_bytes(), &plaintext);
        }
    }
}
