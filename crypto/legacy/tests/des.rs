//! NIST-published, boundary, intermediate, round-trip, and differential DES-family evidence.

use des::{
    Des as ReferenceDes, TdesEde2 as ReferenceTdesEde2, TdesEde3 as ReferenceTdesEde3,
    cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, BlockSizeUser, KeyInit, consts::U8},
};
use rsl_crypto::cipher::BlockCipher as BlockCipherContract;
use rsl_crypto_legacy::{
    SecurityStatus,
    cipher::des::{
        DES_SECURITY_STATUS, Des, DesBlock, DesKey, TRIPLE_DES_SECURITY_STATUS, TripleDesEde2,
        TripleDesEde2Key, TripleDesEde3, TripleDesEde3Key,
    },
};

const K1: [u8; 8] = hex("0123456789abcdef");
const K2: [u8; 8] = hex("23456789abcdef01");
const K3: [u8; 8] = hex("456789abcdef0123");

#[test]
fn nist_tdes_core_first_block_exposes_all_three_ede_stages() {
    let plaintext = hex("6bc1bee22e409f96");
    let mut stage = DesBlock::new(plaintext);

    Des::new(DesKey::new(K1)).encrypt_block(&mut stage);
    assert_eq!(stage.as_bytes(), &hex("7277a00dc1c1c36b"));

    Des::new(DesKey::new(K2)).decrypt_block(&mut stage);
    assert_eq!(stage.as_bytes(), &hex("f6c18af658cc11d5"));

    Des::new(DesKey::new(K3)).encrypt_block(&mut stage);
    assert_eq!(stage.as_bytes(), &hex("714772f339841d34"));
}

#[test]
fn nist_tdes_core_two_key_and_three_key_blocks_round_trip() {
    let plaintexts = [
        hex("6bc1bee22e409f96"),
        hex("e93d7e117393172a"),
        hex("ae2d8a571e03ac9c"),
        hex("9eb76fac45af8e51"),
    ];
    let expected_ede2 = [
        hex("06ede3d82884090a"),
        hex("ff322c19f0518486"),
        hex("730576972a666e58"),
        hex("b6c88cf107340d3d"),
    ];
    let expected_ede3 = [
        hex("714772f339841d34"),
        hex("267fcc4bd2949cc3"),
        hex("ee11c22a576a3038"),
        hex("76183f99c0b6de87"),
    ];
    let ede2 = TripleDesEde2::new(TripleDesEde2Key::new(join2(K1, K2)));
    let ede3 = TripleDesEde3::new(TripleDesEde3Key::new(join3(K1, K2, K3)));

    for index in 0..plaintexts.len() {
        let mut block2 = DesBlock::new(plaintexts[index]);
        ede2.encrypt_block(&mut block2);
        assert_eq!(block2.as_bytes(), &expected_ede2[index]);
        ede2.decrypt_block(&mut block2);
        assert_eq!(block2.as_bytes(), &plaintexts[index]);

        let mut block3 = DesBlock::new(plaintexts[index]);
        apply_contract_round_trip(&ede3, &mut block3);
        assert_eq!(block3.as_bytes(), &plaintexts[index]);
        ede3.encrypt_block(&mut block3);
        assert_eq!(block3.as_bytes(), &expected_ede3[index]);
    }
}

#[test]
fn all_three_variants_match_rustcrypto_over_deterministic_variation() {
    for case in 0_u8..32 {
        let key1 = generated::<8>(case, 0x13);
        let key2 = generated::<8>(case.wrapping_add(0x51), 0x27);
        let key3 = generated::<8>(case.wrapping_add(0xa2), 0x39);
        let plaintext = generated::<8>(case.wrapping_mul(0x47), 0x1d);

        let ours_des = Des::new(DesKey::new(key1));
        let reference_des = ReferenceDes::new(&Array::from(key1));
        compare_one(&ours_des, &reference_des, plaintext, "DES", case);

        let key_ede2 = join2(key1, key2);
        let ours_ede2 = TripleDesEde2::new(TripleDesEde2Key::new(key_ede2));
        let reference_ede2 = ReferenceTdesEde2::new(&Array::from(key_ede2));
        compare_one(&ours_ede2, &reference_ede2, plaintext, "EDE2", case);

        let key_ede3 = join3(key1, key2, key3);
        let ours_ede3 = TripleDesEde3::new(TripleDesEde3Key::new(key_ede3));
        let reference_ede3 = ReferenceTdesEde3::new(&Array::from(key_ede3));
        compare_one(&ours_ede3, &reference_ede3, plaintext, "EDE3", case);
    }
}

#[test]
fn parity_is_visible_but_does_not_gate_historical_reproduction() {
    let odd = DesKey::new(K1);
    assert!(odd.has_odd_parity());

    let wrong_parity = K1.map(|byte| byte ^ 1);
    let changed = DesKey::new(wrong_parity);
    assert!(!changed.has_odd_parity());

    let mut first = DesBlock::new([0x5a; 8]);
    let mut second = DesBlock::new([0x5a; 8]);
    Des::new(odd).encrypt_block(&mut first);
    Des::new(changed).encrypt_block(&mut second);
    assert_eq!(first.as_bytes(), second.as_bytes());
}

#[test]
fn security_status_distinguishes_broken_des_from_withdrawn_tdes() {
    assert_eq!(DES_SECURITY_STATUS, SecurityStatus::Broken);
    assert_eq!(TRIPLE_DES_SECURITY_STATUS, SecurityStatus::Legacy);
    assert_ne!(DES_SECURITY_STATUS, SecurityStatus::Recommended);
    assert_ne!(TRIPLE_DES_SECURITY_STATUS, SecurityStatus::Recommended);
}

fn compare_one<C, R>(ours: &C, reference: &R, plaintext: [u8; 8], name: &str, case: u8)
where
    C: BlockCipherContract<Block = DesBlock>,
    R: BlockCipherEncrypt + BlockCipherDecrypt + BlockSizeUser<BlockSize = U8>,
{
    let mut our_block = DesBlock::new(plaintext);
    let mut reference_block = Array::from(plaintext);
    ours.encrypt_block(&mut our_block);
    reference.encrypt_block(&mut reference_block);
    assert_eq!(
        our_block.as_bytes().as_slice(),
        reference_block.as_slice(),
        "{name} encryption case {case}"
    );
    ours.decrypt_block(&mut our_block);
    reference.decrypt_block(&mut reference_block);
    assert_eq!(our_block.as_bytes(), &plaintext, "{name} case {case}");
    assert_eq!(reference_block.as_slice(), plaintext.as_slice());
}

fn apply_contract_round_trip<C: BlockCipherContract<Block = DesBlock>>(
    cipher: &C,
    block: &mut DesBlock,
) {
    cipher.encrypt_block(block);
    cipher.decrypt_block(block);
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

const fn join2(first: [u8; 8], second: [u8; 8]) -> [u8; 16] {
    let mut output = [0_u8; 16];
    let mut index = 0;
    while index < 8 {
        output[index] = first[index];
        output[index + 8] = second[index];
        index += 1;
    }
    output
}

const fn join3(first: [u8; 8], second: [u8; 8], third: [u8; 8]) -> [u8; 24] {
    let mut output = [0_u8; 24];
    let mut index = 0;
    while index < 8 {
        output[index] = first[index];
        output[index + 8] = second[index];
        output[index + 16] = third[index];
        index += 1;
    }
    output
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
