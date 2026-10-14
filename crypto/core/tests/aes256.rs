//! Public AES-256 validation harness.
//!
//! Provenance: `tests/vectors/aes-256/README.md`. Appendix A.3 key-expansion words are checked
//! white-box beside the implementation.

use aes::{
    Aes256 as ReferenceAes256,
    cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit},
};
use rsl_crypto::cipher::{
    BlockCipher,
    aes::aes256::{Aes256, Aes256Block, Aes256Key},
};

/// The FIPS 197 Appendix A.3 key, which NIST's `AES_Core256.pdf` also uses.
const KEY: [u8; 32] = [
    0x60, 0x3d, 0xeb, 0x10, 0x15, 0xca, 0x71, 0xbe, 0x2b, 0x73, 0xae, 0xf0, 0x85, 0x7d, 0x77, 0x81,
    0x1f, 0x35, 0x2c, 0x07, 0x3b, 0x61, 0x08, 0xd7, 0x2d, 0x98, 0x10, 0xa3, 0x09, 0x14, 0xdf, 0xf4,
];

/// Published evidence: all four `AES_Core256.pdf` ECB blocks encrypt and decrypt.
#[test]
fn nist_core256_all_four_blocks_encrypt_and_decrypt() {
    let cases: [([u8; 16], [u8; 16]); 4] = [
        (
            [
                0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93,
                0x17, 0x2a,
            ],
            [
                0xf3, 0xee, 0xd1, 0xbd, 0xb5, 0xd2, 0xa0, 0x3c, 0x06, 0x4b, 0x5a, 0x7e, 0x3d, 0xb1,
                0x81, 0xf8,
            ],
        ),
        (
            [
                0xae, 0x2d, 0x8a, 0x57, 0x1e, 0x03, 0xac, 0x9c, 0x9e, 0xb7, 0x6f, 0xac, 0x45, 0xaf,
                0x8e, 0x51,
            ],
            [
                0x59, 0x1c, 0xcb, 0x10, 0xd4, 0x10, 0xed, 0x26, 0xdc, 0x5b, 0xa7, 0x4a, 0x31, 0x36,
                0x28, 0x70,
            ],
        ),
        (
            [
                0x30, 0xc8, 0x1c, 0x46, 0xa3, 0x5c, 0xe4, 0x11, 0xe5, 0xfb, 0xc1, 0x19, 0x1a, 0x0a,
                0x52, 0xef,
            ],
            [
                0xb6, 0xed, 0x21, 0xb9, 0x9c, 0xa6, 0xf4, 0xf9, 0xf1, 0x53, 0xe7, 0xb1, 0xbe, 0xaf,
                0xed, 0x1d,
            ],
        ),
        (
            [
                0xf6, 0x9f, 0x24, 0x45, 0xdf, 0x4f, 0x9b, 0x17, 0xad, 0x2b, 0x41, 0x7b, 0xe6, 0x6c,
                0x37, 0x10,
            ],
            [
                0x23, 0x30, 0x4b, 0x7a, 0x39, 0xf9, 0xf3, 0xff, 0x06, 0x7d, 0x8d, 0x8f, 0x9e, 0x24,
                0xec, 0xc7,
            ],
        ),
    ];
    let cipher = Aes256::new(Aes256Key::new(KEY));
    for (index, (plaintext, ciphertext)) in cases.iter().enumerate() {
        let mut block = Aes256Block::new(*plaintext);
        cipher.encrypt_block(&mut block);
        assert_eq!(block.as_bytes(), ciphertext, "block {index} encrypt");
        cipher.decrypt_block(&mut block);
        assert_eq!(block.as_bytes(), plaintext, "block {index} decrypt");
    }
}

/// Differential evidence against the `aes` crate across deterministic keys and blocks.
#[test]
fn encryption_and_decryption_match_rustcrypto() {
    for key_case in 0_u8..24 {
        let key_bytes: [u8; 32] = core::array::from_fn(|index| {
            let index = u8::try_from(index).unwrap();
            key_case
                .wrapping_mul(0x43)
                .wrapping_add(index.wrapping_mul(0x1d))
        });
        let ours = Aes256::new(Aes256Key::new(key_bytes));
        let reference = ReferenceAes256::new(&Array::from(key_bytes));
        for block_case in 0_u8..8 {
            let plaintext: [u8; 16] = core::array::from_fn(|index| {
                let index = u8::try_from(index).unwrap();
                key_case
                    .wrapping_mul(0x71)
                    .wrapping_add(block_case.wrapping_mul(0x2f))
                    .wrapping_add(index.wrapping_mul(0x13))
            });
            let mut ours_block = Aes256Block::new(plaintext);
            let mut reference_block = Array::from(plaintext);
            ours.encrypt_block(&mut ours_block);
            reference.encrypt_block(&mut reference_block);
            assert_eq!(ours_block.as_bytes().as_slice(), reference_block.as_slice());
            BlockCipher::decrypt_block(&ours, &mut ours_block);
            reference.decrypt_block(&mut reference_block);
            assert_eq!(ours_block.as_bytes(), &plaintext);
            assert_eq!(reference_block.as_slice(), &plaintext);
        }
    }
    assert_eq!(
        format!("{:?}", Aes256Key::new([1; 32])),
        "Aes256Key([REDACTED])"
    );
}
