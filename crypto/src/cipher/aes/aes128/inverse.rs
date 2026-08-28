//! Composition of the AES-128 inverse cipher.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 197 §5.3, Algorithm 3][fips-197] defines `INVCIPHER()`. It maps the input block to
//! state, combines the final round key, performs rounds `Nr-1` down through one with
//! `INVSHIFTROWS()`, `INVSUBBYTES()`, `ADDROUNDKEY()`, and `INVMIXCOLUMNS()`, then performs the
//! final inverse row shift, substitution, and round-key-zero addition. Table 3 fixes `Nr = 10`
//! for AES-128.
//!
//! This module owns only that reverse order and the omitted final inverse column mix. Its
//! operations, key schedule, and block/state mapping are independently tested elsewhere.
//! Plaintext is written to the caller's block only after the complete private-state computation.
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

use super::{
    key::{RoundKey, RoundKeySource},
    state::{BLOCK_LEN, State},
    transforms::{add_round_key, inverse_mix_columns, inverse_shift_rows, inverse_sub_bytes},
};

/// Apply one complete inverse round for round indices nine through one.
///
/// **Standard mapping:** FIPS 197 Algorithm 3 lines 5–8 apply these operations in this exact
/// order. `ADDROUNDKEY()` is reused unchanged because §5.3.4 specifies it as its own inverse.
fn apply_complete_inverse_round(state: &mut State, round_key: &RoundKey) {
    inverse_shift_rows(state);
    inverse_sub_bytes(state);
    add_round_key(state, round_key);
    inverse_mix_columns(state);
}

/// Apply the final inverse round without inverse column mixing.
///
/// **Standard mapping:** FIPS 197 Algorithm 3 lines 10–12 perform inverse row shifting, inverse
/// substitution, and key-zero addition. As in the standard, there is no `INVMIXCOLUMNS()` call.
fn apply_final_inverse_round(state: &mut State, round_key: &RoundKey) {
    inverse_shift_rows(state);
    inverse_sub_bytes(state);
    add_round_key(state, round_key);
}

/// Apply FIPS 197 `INVCIPHER()` to one block in place with an expanded AES-128 key.
pub(in crate::cipher::aes) fn decrypt_block<S: RoundKeySource>(
    block: &mut [u8; BLOCK_LEN],
    schedule: &S,
) {
    let mut state = State::from_block(block);
    let final_round_key = schedule.round_key(S::ROUND_COUNT);

    add_round_key(&mut state, &final_round_key);

    for round in (1..S::ROUND_COUNT).rev() {
        let round_key = schedule.round_key(round);
        apply_complete_inverse_round(&mut state, &round_key);
    }

    let initial_round_key = schedule.round_key(0);
    apply_final_inverse_round(&mut state, &initial_round_key);
    state.write_block(block);
}

#[cfg(test)]
mod unit {
    use super::{super::key_schedule::KeySchedule, decrypt_block};
    use crate::cipher::aes::aes128::forward::encrypt_block;

    /// Published complete inverse-cipher evidence from FIPS 197-upd1 Appendix B.
    ///
    /// Appendix B publishes the forward input and output. `INVCIPHER()` must map that published
    /// output back to the published input under the same Appendix A.1 key schedule.
    #[test]
    fn inverse_cipher_recovers_the_appendix_b_input() {
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let mut block = [
            0x39, 0x25, 0x84, 0x1d, 0x02, 0xdc, 0x09, 0xfb, 0xdc, 0x11, 0x85, 0x97, 0x19, 0x6a,
            0x0b, 0x32,
        ];
        let expected = [
            0x32, 0x43, 0xf6, 0xa8, 0x88, 0x5a, 0x30, 0x8d, 0x31, 0x31, 0x98, 0xa2, 0xe0, 0x37,
            0x07, 0x34,
        ];
        let schedule = KeySchedule::expand(&key);

        decrypt_block(&mut block, &schedule);

        assert_eq!(block, expected);
    }

    /// Published inverse-cipher evidence from NIST `AES_Core128.pdf`, first decryption block.
    #[test]
    fn nist_core128_first_ciphertext_recovers_the_published_plaintext() {
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let mut block = [
            0x3a, 0xd7, 0x7b, 0xb4, 0x0d, 0x7a, 0x36, 0x60, 0xa8, 0x9e, 0xca, 0xf3, 0x24, 0x66,
            0xef, 0x97,
        ];
        let expected = [
            0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93,
            0x17, 0x2a,
        ];
        let schedule = KeySchedule::expand(&key);

        decrypt_block(&mut block, &schedule);

        assert_eq!(block, expected);
    }

    /// Standard-derived composition evidence across varied deterministic keys and blocks.
    #[test]
    fn inverse_cipher_cancels_forward_cipher() {
        for case in 0_u8..16 {
            let key = core::array::from_fn(|index| {
                let index = u8::try_from(index).expect("every AES key index fits in u8");
                case.wrapping_mul(0x31)
                    .wrapping_add(index.wrapping_mul(0x17))
            });
            let original = core::array::from_fn(|index| {
                let index = u8::try_from(index).expect("every AES block index fits in u8");
                case.wrapping_mul(0x53)
                    .wrapping_add(index.wrapping_mul(0x29))
            });
            let schedule = KeySchedule::expand(&key);
            let mut block = original;

            encrypt_block(&mut block, &schedule);
            decrypt_block(&mut block, &schedule);

            assert_eq!(block, original, "case {case}");
        }
    }
}
