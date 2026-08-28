//! FIPS 46-3 DES Feistel function and sixteen-round block transform.

use super::{
    constants::{EXPANSION, FINAL_PERMUTATION, INITIAL_PERMUTATION, ROUND_PERMUTATION, S_BOXES},
    permutation::permute,
};

/// Expand, mix with the subkey, substitute through all S-boxes, and permute.
pub(super) fn feistel(right: u32, subkey: u64) -> u32 {
    let expanded = permute(u64::from(right), 32, &EXPANSION);
    let mixed = expanded ^ subkey;
    let mut substituted = 0_u32;

    for (box_index, s_box) in S_BOXES.iter().enumerate() {
        let shift = 42 - (box_index * 6);
        let six_bits = u8::try_from((mixed >> shift) & 0x3f).expect("S-box input is six bits");
        let row = ((six_bits & 0x20) >> 4) | (six_bits & 1);
        let column = (six_bits >> 1) & 0x0f;
        let value = s_box[usize::from(row) * 16 + usize::from(column)];
        substituted = (substituted << 4) | u32::from(value);
    }

    u32::try_from(permute(u64::from(substituted), 32, &ROUND_PERMUTATION))
        .expect("the DES round permutation returns 32 bits")
}

/// Apply initial/final permutations and sixteen Feistel rounds in the selected key order.
pub(super) fn transform(block: &mut [u8; 8], subkeys: &[u64; 16], decrypt: bool) {
    let initial = permute(u64::from_be_bytes(*block), 64, &INITIAL_PERMUTATION);
    let mut left = u32::try_from(initial >> 32).expect("left half contains 32 bits");
    let mut right = u32::try_from(initial & u64::from(u32::MAX)).expect("right half has 32 bits");

    for round in 0..16 {
        let subkey_index = if decrypt { 15 - round } else { round };
        let previous_right = right;
        right = left ^ feistel(right, subkeys[subkey_index]);
        left = previous_right;
    }

    let swapped = (u64::from(right) << 32) | u64::from(left);
    *block = permute(swapped, 64, &FINAL_PERMUTATION).to_be_bytes();
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn classic_first_round_exposes_each_named_value() {
        let right_zero = 0xf0aa_f0aa;
        let first_subkey = 0x1b02_effc_7072;
        let expanded = permute(u64::from(right_zero), 32, &EXPANSION);
        assert_eq!(expanded, 0x7a15_557a_1555);
        assert_eq!(expanded ^ first_subkey, 0x6117_ba86_6527);
        assert_eq!(feistel(right_zero, first_subkey), 0x234a_a9bb);
        assert_eq!(0xcc00_ccff ^ feistel(right_zero, first_subkey), 0xef4a_6544);
    }
}
