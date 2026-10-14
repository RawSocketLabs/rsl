//! Composition of the AES-128 forward cipher.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 197 §5.1, Algorithm 1][fips-197] defines `CIPHER()`. It maps the input block to
//! state, applies round key zero, performs rounds `1..Nr` with `SUBBYTES()`, `SHIFTROWS()`,
//! `MIXCOLUMNS()`, and `ADDROUNDKEY()`, then performs round `Nr` without `MIXCOLUMNS()`. Table 3
//! fixes `Nr = 10` for AES-128. Section 3.4 maps the final state back to output bytes.
//!
//! Every operation is delegated to its independently tested specification layer. This module
//! owns only their order, round indices, and final-round omission. It does not expand keys or
//! define inverse operations. The public algorithm type delegates its forward operation here.
//!
//! The block is overwritten only after all ten rounds complete in a private [`State`]. Temporary
//! state and each copied [`RoundKey`] zeroize on drop. Source-level readability and vector
//! correctness are not claims of compiler-level side-channel resistance.
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

use super::{
    key::{RoundKey, RoundKeySource},
    state::{BLOCK_LEN, State},
    transforms::{add_round_key, mix_columns, shift_rows, sub_bytes},
};

/// Apply one of AES-128's nine complete forward rounds.
///
/// **Standard mapping:** FIPS 197 Algorithm 1 lines 5–8 apply the four transforms in this exact
/// order for round indices one through nine. Naming this complete-round boundary lets tests and
/// later diagnostics distinguish it from the final round, where column mixing is forbidden.
fn apply_complete_round(state: &mut State, round_key: &RoundKey) {
    sub_bytes(state);
    shift_rows(state);
    mix_columns(state);
    add_round_key(state, round_key);
}

/// Apply AES-128's final forward round without column mixing.
///
/// **Standard mapping:** FIPS 197 Algorithm 1 lines 10–12 apply `SUBBYTES()`, `SHIFTROWS()`, and
/// `ADDROUNDKEY()` for `round = Nr`. There is intentionally no call to `mix_columns` here.
fn apply_final_round(state: &mut State, round_key: &RoundKey) {
    sub_bytes(state);
    shift_rows(state);
    add_round_key(state, round_key);
}

/// Transform one 128-bit block in place with an already expanded AES-128 key.
///
/// **Standard mapping:** state loading is Algorithm 1 line 2; round-key zero is line 3; the loop
/// covers rounds one through nine; the named final-round helper covers round ten; and
/// [`State::write_block`] implements line 13 through §3.4 equation 3.7.
pub(in crate::cipher::aes) fn encrypt_block<S: RoundKeySource>(
    block: &mut [u8; BLOCK_LEN],
    schedule: &S,
) {
    let mut state = State::from_block(block);
    let initial_round_key = schedule.round_key(0);

    add_round_key(&mut state, &initial_round_key);

    for round in 1..S::ROUND_COUNT {
        let round_key = schedule.round_key(round);
        apply_complete_round(&mut state, &round_key);
    }

    let final_round_key = schedule.round_key(S::ROUND_COUNT);
    apply_final_round(&mut state, &final_round_key);
    state.write_block(block);
}

#[cfg(test)]
mod unit {
    use super::{super::key_schedule::KeySchedule, encrypt_block};

    /// Published complete-cipher evidence from FIPS 197-upd1 Appendix B.
    #[test]
    fn appendix_b_block_reaches_the_published_output() {
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let mut block = [
            0x32, 0x43, 0xf6, 0xa8, 0x88, 0x5a, 0x30, 0x8d, 0x31, 0x31, 0x98, 0xa2, 0xe0, 0x37,
            0x07, 0x34,
        ];
        let expected = [
            0x39, 0x25, 0x84, 0x1d, 0x02, 0xdc, 0x09, 0xfb, 0xdc, 0x11, 0x85, 0x97, 0x19, 0x6a,
            0x0b, 0x32,
        ];
        let schedule = KeySchedule::expand(&key);

        encrypt_block(&mut block, &schedule);

        assert_eq!(block, expected);
    }

    /// Published supplementary evidence from NIST's `AES_Core128.pdf`, first encryption block.
    ///
    /// The vector uses the same key as FIPS Appendix B with NIST's separate ECB example plaintext
    /// and ciphertext. It therefore checks a second complete input path without becoming a new
    /// algorithm authority.
    #[test]
    fn nist_core128_first_block_reaches_the_published_ciphertext() {
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let mut block = [
            0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93,
            0x17, 0x2a,
        ];
        let expected = [
            0x3a, 0xd7, 0x7b, 0xb4, 0x0d, 0x7a, 0x36, 0x60, 0xa8, 0x9e, 0xca, 0xf3, 0x24, 0x66,
            0xef, 0x97,
        ];
        let schedule = KeySchedule::expand(&key);

        encrypt_block(&mut block, &schedule);

        assert_eq!(block, expected);
    }
}
