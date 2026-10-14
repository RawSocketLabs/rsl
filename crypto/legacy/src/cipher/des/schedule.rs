//! DES's parity-dropping PC-1, 28-bit rotations, and PC-2 subkey selection.

use super::{
    constants::{KEY_ROTATIONS, PERMUTED_CHOICE_ONE, PERMUTED_CHOICE_TWO},
    permutation::permute,
};

const HALF_MASK: u32 = 0x0fff_ffff;

/// Rotate one 28-bit key-schedule half without allowing bits into positions 28–31.
const fn rotate_28(value: u32, amount: u8) -> u32 {
    ((value << amount) | (value >> (28 - amount))) & HALF_MASK
}

/// Expand one encoded 64-bit DES key into sixteen 48-bit round subkeys.
// A borrow avoids creating an extra secret-key copy for an optimization that does not matter to
// this accuracy-first key-setup path.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub(super) fn expand(key: &[u8; 8]) -> [u64; 16] {
    let encoded_key = u64::from_be_bytes(*key);
    let selected = permute(encoded_key, 64, &PERMUTED_CHOICE_ONE);
    let mut c = u32::try_from(selected >> 28).expect("PC-1 half contains 28 bits");
    let mut d = u32::try_from(selected & u64::from(HALF_MASK)).expect("PC-1 half contains 28 bits");
    let mut subkeys = [0_u64; 16];

    for (round, rotation) in KEY_ROTATIONS.into_iter().enumerate() {
        c = rotate_28(c, rotation);
        d = rotate_28(d, rotation);
        let joined = (u64::from(c) << 28) | u64::from(d);
        subkeys[round] = permute(joined, 56, &PERMUTED_CHOICE_TWO);
    }
    subkeys
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn classic_key_schedule_has_published_first_and_last_subkeys() {
        let subkeys = expand(&0x1334_5779_9bbc_dff1_u64.to_be_bytes());
        assert_eq!(subkeys[0], 0x1b02_effc_7072);
        assert_eq!(subkeys[15], 0xcb3d_8b0e_17f5);
    }

    #[test]
    fn parity_bits_do_not_affect_any_subkey() {
        let key = 0x1334_5779_9bbc_dff1_u64.to_be_bytes();
        let changed_parity = key.map(|byte| byte ^ 1);
        assert_eq!(expand(&key), expand(&changed_parity));
    }
}
