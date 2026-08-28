//! NIST-published, stateful, malformed-boundary, and differential CBC evidence.

use des::{
    TdesEde3 as ReferenceTdesEde3,
    cipher::{Array, BlockCipherEncrypt, KeyInit},
};
use rsl_crypto::cipher::BlockCipher;
use rsl_crypto_legacy::{
    CryptoError, SecurityStatus,
    cipher::{
        cbc::{CbcState, SECURITY_STATUS, decrypt_blocks, encrypt_blocks},
        des::{DesBlock, TripleDesEde3, TripleDesEde3Key},
    },
};

const KEY: [u8; 24] = hex("0123456789abcdef23456789abcdef01456789abcdef0123");
const IV: [u8; 8] = hex("f69f2445df4f9b17");
const PLAINTEXT: [[u8; 8]; 4] = [
    hex("6bc1bee22e409f96"),
    hex("e93d7e117393172a"),
    hex("ae2d8a571e03ac9c"),
    hex("9eb76fac45af8e51"),
];
const CIPHERTEXT: [[u8; 8]; 4] = [
    hex("2079c3d53aa763e1"),
    hex("93b79e2569ab5262"),
    hex("516570481f25b50f"),
    hex("73c0bda85c8e0da7"),
];

#[test]
fn nist_tdes_cbc_four_blocks_encrypt_decrypt_and_advance_chain() {
    let cipher = TripleDesEde3::new(TripleDesEde3Key::new(KEY));
    let mut blocks = PLAINTEXT.map(DesBlock::new);
    let mut encryption_state = CbcState::new(DesBlock::new(IV));

    encrypt_blocks(&cipher, &mut encryption_state, &mut blocks).unwrap();
    for (block, expected) in blocks.iter().zip(CIPHERTEXT) {
        assert_eq!(block.as_bytes(), &expected);
    }
    assert_eq!(encryption_state.as_bytes(), &CIPHERTEXT[3]);

    let mut decryption_state = CbcState::new(DesBlock::new(IV));
    decrypt_blocks(&cipher, &mut decryption_state, &mut blocks).unwrap();
    for (block, expected) in blocks.iter().zip(PLAINTEXT) {
        assert_eq!(block.as_bytes(), &expected);
    }
    assert_eq!(decryption_state.as_bytes(), &CIPHERTEXT[3]);
}

#[test]
fn multiple_calls_are_one_continuous_chain() {
    let cipher = TripleDesEde3::new(TripleDesEde3Key::new(KEY));
    let mut one_shot = PLAINTEXT.map(DesBlock::new);
    let mut fragmented = PLAINTEXT.map(DesBlock::new);
    let mut one_state = CbcState::new(DesBlock::new(IV));
    let mut fragmented_state = CbcState::new(DesBlock::new(IV));

    encrypt_blocks(&cipher, &mut one_state, &mut one_shot).unwrap();
    let (first, rest) = fragmented.split_at_mut(1);
    encrypt_blocks(&cipher, &mut fragmented_state, first).unwrap();
    encrypt_blocks(&cipher, &mut fragmented_state, rest).unwrap();

    for (whole, split) in one_shot.iter().zip(&fragmented) {
        assert_eq!(whole.as_bytes(), split.as_bytes());
    }
    assert_eq!(one_state.as_bytes(), fragmented_state.as_bytes());
}

#[test]
fn deterministic_cbc_sequences_match_independent_rustcrypto_des() {
    for case in 0_u8..16 {
        let key = generated::<24>(case.wrapping_mul(17), 29);
        let iv = generated::<8>(case.wrapping_mul(31), 7);
        let plaintext: [[u8; 8]; 6] = core::array::from_fn(|block| {
            generated::<8>(
                case.wrapping_add(block.to_le_bytes()[0].wrapping_mul(43)),
                11,
            )
        });

        let cipher = TripleDesEde3::new(TripleDesEde3Key::new(key));
        let mut ours = plaintext.map(DesBlock::new);
        let mut state = CbcState::new(DesBlock::new(iv));
        encrypt_blocks(&cipher, &mut state, &mut ours).unwrap();

        let reference = ReferenceTdesEde3::new(&Array::from(key));
        let expected = reference_encrypt_cbc(&reference, iv, plaintext);
        for (actual, expected) in ours.iter().zip(expected) {
            assert_eq!(actual.as_bytes(), &expected, "case {case}");
        }
        assert_eq!(state.as_bytes(), &expected_last(&reference, iv, plaintext));
    }
}

#[test]
fn malformed_custom_block_lengths_are_rejected_before_mutation() {
    let cipher = IdentityCipher;
    let mut state = CbcState::new(VariableBlock(vec![0x11; 8]));
    let mut blocks = [VariableBlock(vec![0x22; 7]), VariableBlock(vec![0x33; 8])];

    assert_eq!(
        encrypt_blocks(&cipher, &mut state, &mut blocks),
        Err(CryptoError::InvalidLength {
            name: "CBC block",
            expected: 8,
            actual: 7,
        })
    );
    assert_eq!(state.as_bytes(), &[0x11; 8]);
    assert_eq!(blocks[0].as_ref(), &[0x22; 7]);
    assert_eq!(blocks[1].as_ref(), &[0x33; 8]);
}

#[test]
fn empty_sequences_leave_state_unchanged_and_status_is_legacy() {
    let cipher = TripleDesEde3::new(TripleDesEde3Key::new(KEY));
    let mut state = CbcState::new(DesBlock::new(IV));
    encrypt_blocks(&cipher, &mut state, &mut []).unwrap();
    decrypt_blocks(&cipher, &mut state, &mut []).unwrap();
    assert_eq!(state.as_bytes(), &IV);
    assert_eq!(SECURITY_STATUS, SecurityStatus::Legacy);
    assert_ne!(SECURITY_STATUS, SecurityStatus::Recommended);
}

fn reference_encrypt_cbc(
    cipher: &ReferenceTdesEde3,
    iv: [u8; 8],
    mut blocks: [[u8; 8]; 6],
) -> [[u8; 8]; 6] {
    let mut chain = iv;
    for block in &mut blocks {
        for (byte, chain_byte) in block.iter_mut().zip(chain) {
            *byte ^= chain_byte;
        }
        let mut oracle_block = Array::from(*block);
        cipher.encrypt_block(&mut oracle_block);
        *block = oracle_block.into();
        chain = *block;
    }
    blocks
}

fn expected_last(cipher: &ReferenceTdesEde3, iv: [u8; 8], blocks: [[u8; 8]; 6]) -> [u8; 8] {
    reference_encrypt_cbc(cipher, iv, blocks)[5]
}

const fn generated<const N: usize>(seed: u8, step: u8) -> [u8; N] {
    let mut output = [0_u8; N];
    let mut index = 0;
    while index < N {
        output[index] = seed.wrapping_add(index.to_le_bytes()[0].wrapping_mul(step));
        index += 1;
    }
    output
}

struct VariableBlock(Vec<u8>);

impl AsRef<[u8]> for VariableBlock {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsMut<[u8]> for VariableBlock {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

struct IdentityCipher;

impl BlockCipher for IdentityCipher {
    type Block = VariableBlock;

    fn encrypt_block(&self, _block: &mut Self::Block) {}

    fn decrypt_block(&self, _block: &mut Self::Block) {}
}

const fn hex<const N: usize>(input: &str) -> [u8; N] {
    let bytes = input.as_bytes();
    let mut output = [0_u8; N];
    let mut index = 0;
    while index < N {
        output[index] = (nibble(bytes[index * 2]) << 4) | nibble(bytes[index * 2 + 1]);
        index += 1;
    }
    output
}

const fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => panic!("fixture must use lowercase hexadecimal"),
    }
}
