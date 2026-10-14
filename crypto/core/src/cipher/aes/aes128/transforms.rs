//! Forward AES transformations of the complete state.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 197 §5.1.1][fips-197] defines `SUBBYTES()` as the independent application of
//! `SBOX()` to each byte `s[r, c]` in the state. Section 5.1.2 and equation 5.5 define
//! `SHIFTROWS()` as a cyclic left shift of row `r` by `r` positions. Section 5.1.3, equations
//! 5.6–5.8, define `MIXCOLUMNS()` using a fixed matrix over `GF(2^8)`. Section 5.1.4 and equation
//! 5.9 define `ADDROUNDKEY()` as the word-by-word XOR of a round key into corresponding state
//! columns. Sections 5.3.1–5.3.3 define inverse row shifting, inverse byte substitution, and the
//! inverse column matrix. Section 5.3.4 reuses `ADDROUNDKEY()` because XOR is self-inverse.
//!
//! The byte substitution calculation belongs to `substitution`; this layer owns only complete
//! state traversal. Explicit row and column loops retain the standard's `s[r, c]` coordinate
//! order and make coverage of all sixteen positions visible.
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

use super::{
    field::{add, multiply},
    key::RoundKey,
    state::{STATE_COLUMNS, STATE_ROWS, State},
    substitution::{inverse_substitute_byte, substitute_byte},
};
use zeroize::Zeroize;

/// Field element `{02}` in FIPS 197 equations 5.6–5.8.
const MIX_TIMES_TWO: u8 = 0x02;

/// Field element `{03}` in FIPS 197 equations 5.6–5.8.
const MIX_TIMES_THREE: u8 = 0x03;

/// First coefficient `{0e}` of FIPS 197 equation 5.13's inverse column word.
const INVERSE_MIX_TIMES_FOURTEEN: u8 = 0x0e;

/// Second matrix coefficient `{0b}` appearing in FIPS 197 equations 5.14–5.15.
const INVERSE_MIX_TIMES_ELEVEN: u8 = 0x0b;

/// Third matrix coefficient `{0d}` appearing in FIPS 197 equations 5.14–5.15.
const INVERSE_MIX_TIMES_THIRTEEN: u8 = 0x0d;

/// Fourth matrix coefficient `{09}` appearing in FIPS 197 equations 5.14–5.15.
const INVERSE_MIX_TIMES_NINE: u8 = 0x09;

/// Add four `GF(2^8)` terms while keeping equation 5.8's four contributions visible.
///
/// Field addition is associative bitwise XOR, but the two pairs make it easier to compare each
/// caller with one printed output-byte equation.
#[must_use]
const fn add_four(first: u8, second: u8, third: u8, fourth: u8) -> u8 {
    add(add(first, second), add(third, fourth))
}

/// Multiply one four-byte state column by the fixed forward AES matrix.
///
/// **Standard mapping:** FIPS 197 §5.1.3 equation 5.6 supplies the matrix coefficients `{02}`,
/// `{01}`, `{01}`, and `{03}`. The four expressions below are equation 5.8 in row order. Terms
/// with coefficient `{01}` remain visibly unchanged; terms with `{02}` or `{03}` call the field
/// multiplication layer.
///
/// The input is a private copy of one state column and is zeroized after all four outputs have
/// been calculated.
#[must_use]
fn mix_column(mut input: [u8; STATE_ROWS]) -> [u8; STATE_ROWS] {
    let mixed = [
        add_four(
            multiply(MIX_TIMES_TWO, input[0]),
            multiply(MIX_TIMES_THREE, input[1]),
            input[2],
            input[3],
        ),
        add_four(
            input[0],
            multiply(MIX_TIMES_TWO, input[1]),
            multiply(MIX_TIMES_THREE, input[2]),
            input[3],
        ),
        add_four(
            input[0],
            input[1],
            multiply(MIX_TIMES_TWO, input[2]),
            multiply(MIX_TIMES_THREE, input[3]),
        ),
        add_four(
            multiply(MIX_TIMES_THREE, input[0]),
            input[1],
            input[2],
            multiply(MIX_TIMES_TWO, input[3]),
        ),
    ];

    input.zeroize();
    mixed
}

/// Multiply one state column by the fixed inverse AES matrix.
///
/// **Standard mapping:** FIPS 197 §5.3.3 equation 5.13 supplies coefficients `{0e}`, `{09}`,
/// `{0d}`, and `{0b}`; equation 5.15 expands the matrix product into these four output-byte
/// equations. All terms use the calculation-based field multiplier. The input copy is zeroized
/// after the output has been calculated.
#[must_use]
fn inverse_mix_column(mut input: [u8; STATE_ROWS]) -> [u8; STATE_ROWS] {
    let mixed = [
        add_four(
            multiply(INVERSE_MIX_TIMES_FOURTEEN, input[0]),
            multiply(INVERSE_MIX_TIMES_ELEVEN, input[1]),
            multiply(INVERSE_MIX_TIMES_THIRTEEN, input[2]),
            multiply(INVERSE_MIX_TIMES_NINE, input[3]),
        ),
        add_four(
            multiply(INVERSE_MIX_TIMES_NINE, input[0]),
            multiply(INVERSE_MIX_TIMES_FOURTEEN, input[1]),
            multiply(INVERSE_MIX_TIMES_ELEVEN, input[2]),
            multiply(INVERSE_MIX_TIMES_THIRTEEN, input[3]),
        ),
        add_four(
            multiply(INVERSE_MIX_TIMES_THIRTEEN, input[0]),
            multiply(INVERSE_MIX_TIMES_NINE, input[1]),
            multiply(INVERSE_MIX_TIMES_FOURTEEN, input[2]),
            multiply(INVERSE_MIX_TIMES_ELEVEN, input[3]),
        ),
        add_four(
            multiply(INVERSE_MIX_TIMES_ELEVEN, input[0]),
            multiply(INVERSE_MIX_TIMES_THIRTEEN, input[1]),
            multiply(INVERSE_MIX_TIMES_NINE, input[2]),
            multiply(INVERSE_MIX_TIMES_FOURTEEN, input[3]),
        ),
    ];

    input.zeroize();
    mixed
}

/// Apply the forward AES S-box independently to every state byte.
///
/// **Standard mapping:** FIPS 197 §5.1.1 states that `SUBBYTES()` replaces each `s[r, c]` with
/// `SBOX(s[r, c])`. The nested loops visit every permitted `r` and `c` exactly once. Reading the
/// old byte into a named value before replacement makes the non-crossing, byte-independent nature
/// of this transformation explicit.
pub(in crate::cipher::aes) fn sub_bytes(state: &mut State) {
    for row in 0..STATE_ROWS {
        for column in 0..STATE_COLUMNS {
            let original_byte = state.byte(row, column);
            let substituted_byte = substitute_byte(original_byte);

            state.set_byte(row, column, substituted_byte);
        }
    }
}

/// Apply the inverse AES S-box independently to every state byte.
///
/// **Standard mapping:** FIPS 197 §5.3.2 defines `INVSUBBYTES()` as replacement of every state
/// byte with `INVSBOX()`. The traversal mirrors [`sub_bytes`] and delegates the individual
/// calculation to the exhaustively Table-6-tested inverse substitution layer.
pub(in crate::cipher::aes) fn inverse_sub_bytes(state: &mut State) {
    for row in 0..STATE_ROWS {
        for column in 0..STATE_COLUMNS {
            let original_byte = state.byte(row, column);
            let substituted_byte = inverse_substitute_byte(original_byte);

            state.set_byte(row, column, substituted_byte);
        }
    }
}

/// Cyclically shift each state row left by its row index.
///
/// **Standard mapping:** FIPS 197 §5.1.2, equation 5.5 defines the new byte as
/// `s'[r, c] = s[r, (c + r) mod 4]`. `original_row` preserves all four inputs while their output
/// positions are replaced. The inner loop assigns that equation literally for every column; row
/// zero therefore remains unchanged, while rows one through three move left by one through three
/// positions.
///
/// The temporary row may contain sensitive intermediate state and is explicitly zeroized after
/// its four output positions have been written.
pub(in crate::cipher::aes) fn shift_rows(state: &mut State) {
    for row in 0..STATE_ROWS {
        let mut original_row: [u8; STATE_COLUMNS] =
            core::array::from_fn(|column| state.byte(row, column));

        for column in 0..STATE_COLUMNS {
            let source_column = (column + row) % STATE_COLUMNS;
            state.set_byte(row, column, original_row[source_column]);
        }

        original_row.zeroize();
    }
}

/// Cyclically shift each state row right by its row index.
///
/// **Standard mapping:** FIPS 197 §5.3.1, equation 5.12 defines
/// `s'[r,c] = s[r,(c-r) mod 4]`. Adding four before subtracting `row` keeps the Rust `usize`
/// expression nonnegative; the final remainder implements the standard's modulo-four index.
/// Each temporary row is zeroized after all four outputs are installed.
pub(in crate::cipher::aes) fn inverse_shift_rows(state: &mut State) {
    for row in 0..STATE_ROWS {
        let mut original_row: [u8; STATE_COLUMNS] =
            core::array::from_fn(|column| state.byte(row, column));

        for column in 0..STATE_COLUMNS {
            let source_column = (column + STATE_COLUMNS - row) % STATE_COLUMNS;
            state.set_byte(row, column, original_row[source_column]);
        }

        original_row.zeroize();
    }
}

/// Apply the fixed forward AES matrix independently to every state column.
///
/// **Standard mapping:** FIPS 197 §5.1.3 states that `MIXCOLUMNS()` transforms all four columns
/// independently. Each `original_column` is read in `s[0,c]` through `s[3,c]` order required by
/// equation 5.8, passed to [`mix_column`], and written back at the same column index. No byte from
/// a newly mixed column can affect a later column.
///
/// Both temporary input and output columns may contain sensitive intermediate values. The helper
/// zeroizes the input copy, and this function zeroizes the output copy after writing it to state.
pub(in crate::cipher::aes) fn mix_columns(state: &mut State) {
    for column in 0..STATE_COLUMNS {
        let original_column: [u8; STATE_ROWS] = core::array::from_fn(|row| state.byte(row, column));
        let mut mixed_column = mix_column(original_column);

        for (row, mixed_byte) in mixed_column.iter().copied().enumerate() {
            state.set_byte(row, column, mixed_byte);
        }

        mixed_column.zeroize();
    }
}

/// Apply the fixed inverse AES matrix independently to every state column.
///
/// **Standard mapping:** FIPS 197 §5.3.3 equations 5.13–5.15 give the inverse matrix and its four
/// output equations. Column traversal and temporary zeroization mirror [`mix_columns`], while
/// [`inverse_mix_column`] owns the different coefficients.
pub(in crate::cipher::aes) fn inverse_mix_columns(state: &mut State) {
    for column in 0..STATE_COLUMNS {
        let original_column: [u8; STATE_ROWS] = core::array::from_fn(|row| state.byte(row, column));
        let mut mixed_column = inverse_mix_column(original_column);

        for (row, mixed_byte) in mixed_column.iter().copied().enumerate() {
            state.set_byte(row, column, mixed_byte);
        }

        mixed_column.zeroize();
    }
}

/// Combine one four-word round key with the corresponding four state columns.
///
/// **Standard mapping:** FIPS 197 §5.1.4, equation 5.9 combines state column `c` with key-schedule
/// word `w[4 * round + c]`. A [`RoundKey`] already represents the four selected words, so
/// `round_key.byte(column, row)` names the byte aligned with `s[row, column]` by §3.5 equation
/// 3.8. Field addition is bytewise XOR through [`add`].
///
/// Every state position is replaced exactly once. The round key is borrowed and unchanged so one
/// expanded schedule can protect multiple blocks later.
pub(in crate::cipher::aes) fn add_round_key(state: &mut State, round_key: &RoundKey) {
    for column in 0..STATE_COLUMNS {
        for row in 0..STATE_ROWS {
            let state_byte = state.byte(row, column);
            let key_byte = round_key.byte(column, row);

            state.set_byte(row, column, add(state_byte, key_byte));
        }
    }
}

#[cfg(test)]
mod unit {
    use super::{
        RoundKey, State, add_round_key, inverse_mix_column, inverse_mix_columns,
        inverse_shift_rows, inverse_sub_bytes, mix_column, mix_columns, shift_rows, sub_bytes,
    };

    /// Assert that a state serializes to one expected FIPS byte sequence.
    fn assert_block(state: &State, expected: [u8; 16]) {
        let mut actual = [0_u8; 16];
        state.write_block(&mut actual);
        assert_eq!(actual, expected);
    }

    /// Published intermediate-state evidence from FIPS 197-upd1 Appendix B, round 1.
    ///
    /// Appendix B prints both matrices directly: the first is the state at the start of round 1,
    /// and the second is the state after `SUBBYTES()`. The one-dimensional input below is the
    /// first printed matrix converted mechanically through §3.4 equation 3.7.
    #[test]
    fn round_one_sub_bytes_matches_the_published_appendix_b_state() {
        let start_of_round = [
            0x19, 0x3d, 0xe3, 0xbe, 0xa0, 0xf4, 0xe2, 0x2b, 0x9a, 0xc6, 0x8d, 0x2a, 0xe9, 0xf8,
            0x48, 0x08,
        ];
        let expected_rows = [
            [0xd4, 0xe0, 0xb8, 0x1e],
            [0x27, 0xbf, 0xb4, 0x41],
            [0x11, 0x98, 0x5d, 0x52],
            [0xae, 0xf1, 0xe5, 0x30],
        ];
        let mut state = State::from_block(&start_of_round);

        sub_bytes(&mut state);

        for (row, expected_row) in expected_rows.into_iter().enumerate() {
            for (column, expected_byte) in expected_row.into_iter().enumerate() {
                assert_eq!(
                    state.byte(row, column),
                    expected_byte,
                    "state byte s[{row}, {column}]"
                );
            }
        }
    }

    /// Published intermediate-state evidence from FIPS 197-upd1 Appendix B, round 1.
    ///
    /// The starting and expected matrices are Appendix B's post-`SUBBYTES()` and post-`SHIFTROWS()`
    /// columns. The input byte sequence is the first matrix converted through §3.4 equation 3.7;
    /// expected values remain arranged exactly as the four published rows.
    #[test]
    fn round_one_shift_rows_matches_the_published_appendix_b_state() {
        let after_sub_bytes = [
            0xd4, 0x27, 0x11, 0xae, 0xe0, 0xbf, 0x98, 0xf1, 0xb8, 0xb4, 0x5d, 0xe5, 0x1e, 0x41,
            0x52, 0x30,
        ];
        let expected_rows = [
            [0xd4, 0xe0, 0xb8, 0x1e],
            [0xbf, 0xb4, 0x41, 0x27],
            [0x5d, 0x52, 0x11, 0x98],
            [0x30, 0xae, 0xf1, 0xe5],
        ];
        let mut state = State::from_block(&after_sub_bytes);

        shift_rows(&mut state);

        for (row, expected_row) in expected_rows.into_iter().enumerate() {
            for (column, expected_byte) in expected_row.into_iter().enumerate() {
                assert_eq!(
                    state.byte(row, column),
                    expected_byte,
                    "state byte s[{row}, {column}]"
                );
            }
        }
    }

    /// Published first-column evidence from FIPS 197-upd1 Appendix B, round 1.
    ///
    /// Appendix B's post-`SHIFTROWS()` first column is `{d4,bf,5d,30}` and its post-`MIXCOLUMNS()`
    /// first column is `{04,66,81,e5}`. Isolating that pair tests equation 5.8 without relying on
    /// state traversal.
    #[test]
    fn round_one_first_mixed_column_matches_appendix_b() {
        assert_eq!(
            mix_column([0xd4, 0xbf, 0x5d, 0x30]),
            [0x04, 0x66, 0x81, 0xe5]
        );
    }

    /// Published complete-state evidence from FIPS 197-upd1 Appendix B, round 1.
    ///
    /// The input and expected matrices are the published post-`SHIFTROWS()` and
    /// post-`MIXCOLUMNS()` boundaries. This separately checks that all four columns are selected,
    /// transformed, and returned at the correct indices.
    #[test]
    fn round_one_mix_columns_matches_the_published_appendix_b_state() {
        let after_shift_rows = [
            0xd4, 0xbf, 0x5d, 0x30, 0xe0, 0xb4, 0x52, 0xae, 0xb8, 0x41, 0x11, 0xf1, 0x1e, 0x27,
            0x98, 0xe5,
        ];
        let expected_rows = [
            [0x04, 0xe0, 0x48, 0x28],
            [0x66, 0xcb, 0xf8, 0x06],
            [0x81, 0x19, 0xd3, 0x26],
            [0xe5, 0x9a, 0x7a, 0x4c],
        ];
        let mut state = State::from_block(&after_shift_rows);

        mix_columns(&mut state);

        for (row, expected_row) in expected_rows.into_iter().enumerate() {
            for (column, expected_byte) in expected_row.into_iter().enumerate() {
                assert_eq!(
                    state.byte(row, column),
                    expected_byte,
                    "state byte s[{row}, {column}]"
                );
            }
        }
    }

    /// Published state/key boundary from FIPS 197-upd1 Appendix B, initial key addition.
    ///
    /// Appendix B prints the input matrix, cipher key matrix, and round-1 start matrix. The
    /// transformation below combines exactly those first two published values; each expected row
    /// is the third published value.
    #[test]
    fn initial_add_round_key_matches_the_published_appendix_b_state() {
        let input = [
            0x32, 0x43, 0xf6, 0xa8, 0x88, 0x5a, 0x30, 0x8d, 0x31, 0x31, 0x98, 0xa2, 0xe0, 0x37,
            0x07, 0x34,
        ];
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let expected_rows = [
            [0x19, 0xa0, 0x9a, 0xe9],
            [0x3d, 0xf4, 0xc6, 0xf8],
            [0xe3, 0xe2, 0x8d, 0x48],
            [0xbe, 0x2b, 0x2a, 0x08],
        ];
        let mut state = State::from_block(&input);
        let round_key = RoundKey::from_block(&key);

        add_round_key(&mut state, &round_key);

        for (row, expected_row) in expected_rows.into_iter().enumerate() {
            for (column, expected_byte) in expected_row.into_iter().enumerate() {
                assert_eq!(
                    state.byte(row, column),
                    expected_byte,
                    "state byte s[{row}, {column}]"
                );
            }
        }
    }

    /// Standard-derived inverse-property evidence from FIPS 197-upd1 §5.3.4.
    ///
    /// The standard states that `ADDROUNDKEY()` is its own inverse. Applying the same round key
    /// twice must therefore recover all sixteen original bytes.
    #[test]
    fn add_round_key_is_its_own_inverse() {
        let input = core::array::from_fn(|index| {
            u8::try_from(index).expect("every AES block index fits in u8")
        });
        let key = [0xa5; 16];
        let mut state = State::from_block(&input);
        let round_key = RoundKey::from_block(&key);
        let mut output = [0_u8; 16];

        add_round_key(&mut state, &round_key);
        add_round_key(&mut state, &round_key);
        state.write_block(&mut output);

        assert_eq!(output, input);
    }

    /// Published inverse-substitution evidence from NIST `AES_Core128.pdf`, decryption block 1.
    #[test]
    fn inverse_sub_bytes_matches_the_first_published_decryption_boundary() {
        let after_initial_key_addition = [
            0xea, 0xc3, 0x82, 0x1c, 0xc4, 0x94, 0x13, 0xe9, 0x49, 0xa1, 0xc6, 0x3b, 0x92, 0x05,
            0xe3, 0x31,
        ];
        let expected = [
            0xbb, 0x33, 0x11, 0xc4, 0x88, 0xe7, 0x82, 0xeb, 0xa4, 0xf1, 0xc7, 0x49, 0x74, 0x36,
            0x4d, 0x2e,
        ];
        let mut state = State::from_block(&after_initial_key_addition);

        inverse_sub_bytes(&mut state);

        assert_block(&state, expected);
    }

    /// Published inverse-row evidence from NIST `AES_Core128.pdf`, decryption block 1.
    #[test]
    fn inverse_shift_rows_matches_the_first_published_decryption_boundary() {
        let after_inverse_substitution = [
            0xbb, 0x33, 0x11, 0xc4, 0x88, 0xe7, 0x82, 0xeb, 0xa4, 0xf1, 0xc7, 0x49, 0x74, 0x36,
            0x4d, 0x2e,
        ];
        let expected = [
            0xbb, 0x36, 0xc7, 0xeb, 0x88, 0x33, 0x4d, 0x49, 0xa4, 0xe7, 0x11, 0x2e, 0x74, 0xf1,
            0x82, 0xc4,
        ];
        let mut state = State::from_block(&after_inverse_substitution);

        inverse_shift_rows(&mut state);

        assert_block(&state, expected);
    }

    /// Published inverse-column evidence from NIST `AES_Core128.pdf`, decryption round 9.
    #[test]
    fn inverse_mix_columns_matches_the_first_published_decryption_round() {
        let after_round_key = [
            0x17, 0x41, 0xa1, 0x18, 0x91, 0xc9, 0x91, 0x68, 0x8c, 0x36, 0x38, 0x6f, 0x23, 0xad,
            0x82, 0xaa,
        ];
        let expected = [
            0x83, 0x33, 0xf0, 0xaf, 0xff, 0x15, 0xa6, 0xed, 0xc1, 0x91, 0xb4, 0x09, 0x77, 0x0e,
            0x81, 0x5e,
        ];
        let mut state = State::from_block(&after_round_key);

        inverse_mix_columns(&mut state);

        assert_block(&state, expected);
    }

    /// Standard-derived inverse-property evidence from FIPS 197 §§5.1.1–5.1.3 and §§5.3.1–5.3.3.
    #[test]
    fn every_inverse_state_transform_cancels_its_forward_transform() {
        let input = core::array::from_fn(|index| {
            let index = u8::try_from(index).expect("every AES block index fits in u8");
            index.wrapping_mul(0x11).wrapping_add(0x07)
        });

        let mut substitution_state = State::from_block(&input);
        sub_bytes(&mut substitution_state);
        inverse_sub_bytes(&mut substitution_state);
        assert_block(&substitution_state, input);

        let mut row_state = State::from_block(&input);
        shift_rows(&mut row_state);
        inverse_shift_rows(&mut row_state);
        assert_block(&row_state, input);

        let mut column_state = State::from_block(&input);
        mix_columns(&mut column_state);
        inverse_mix_columns(&mut column_state);
        assert_block(&column_state, input);

        for column_start in (0..16).step_by(4) {
            let column = [
                input[column_start],
                input[column_start + 1],
                input[column_start + 2],
                input[column_start + 3],
            ];
            assert_eq!(inverse_mix_column(mix_column(column)), column);
        }
    }
}
