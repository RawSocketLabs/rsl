//! Public boundary and reversible-composition evidence.

use rsl_crypto::cipher::aes::aes128::{Aes128, Aes128Block, Aes128Key};

/// Standard-derived evidence over deterministic key and block variation.
#[test]
fn varied_blocks_round_trip_under_varied_keys() {
    for key_case in 0_u8..16 {
        let key_bytes = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every AES key index fits in u8");
            key_case
                .wrapping_mul(0x3d)
                .wrapping_add(index.wrapping_mul(0x17))
        });
        let cipher = Aes128::new(Aes128Key::new(key_bytes));

        for block_case in 0_u8..8 {
            let plaintext = core::array::from_fn(|index| {
                let index = u8::try_from(index).expect("every AES block index fits in u8");
                key_case
                    .wrapping_mul(0x53)
                    .wrapping_add(block_case.wrapping_mul(0x29))
                    .wrapping_add(index.wrapping_mul(0x0b))
            });
            let mut block = Aes128Block::new(plaintext);

            cipher.encrypt_block(&mut block);
            assert_ne!(
                block.as_bytes(),
                &plaintext,
                "key {key_case}, block {block_case}"
            );

            cipher.decrypt_block(&mut block);
            assert_eq!(
                block.as_bytes(),
                &plaintext,
                "key {key_case}, block {block_case}"
            );
        }
    }
}

/// API-regression evidence that the generic block-cipher contract uses the same public path.
#[test]
fn generic_block_cipher_contract_dispatches_both_directions() {
    use rsl_crypto::cipher::BlockCipher;

    fn round_trip<C: BlockCipher>(cipher: &C, block: &mut C::Block) {
        cipher.encrypt_block(block);
        cipher.decrypt_block(block);
    }

    let plaintext = [0x5c; 16];
    let cipher = Aes128::new(Aes128Key::new([0xa7; 16]));
    let mut block = Aes128Block::new(plaintext);

    round_trip(&cipher, &mut block);

    assert_eq!(block.as_bytes(), &plaintext);
}
