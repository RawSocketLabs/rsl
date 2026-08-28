//! Mapping between an AES block and the algorithm's two-dimensional state.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 197 §3.1][fips-197] fixes every AES input and output block at 128 bits, or sixteen
//! bytes. Section 3.4 defines the internal state as four rows by four columns. Equation 3.6 maps
//! input byte `in[r + 4c]` to `s[r, c]`; equation 3.7 maps `s[r, c]` back to output byte
//! `out[r + 4c]`. Figure 1 prints the same column-major wire relationship visually.
//!
//! This module owns only that reversible representation boundary. It does not implement any
//! §5 round transformation or interpret a block as a key. Storing explicit `rows[row][column]`
//! makes later equations readable with the same `(r, c)` order used by the standard, even though
//! input and output byte sequences advance down a column before moving to the next column.
//!
//! The state may temporarily contain sensitive plaintext or key-dependent values, so its private
//! copy is zeroized on drop. The caller continues to own the input/output block and its lifetime.
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

use zeroize::Zeroize;

/// Number of rows in the AES state.
///
/// FIPS 197 §3.4 fixes row index `r` to `0 <= r < 4`.
pub(super) const STATE_ROWS: usize = 4;

/// Number of columns in the AES state.
///
/// FIPS 197 §2.3 assigns `Nb = 4`, and §3.4 fixes column index `c` to `0 <= c < 4`.
pub(super) const STATE_COLUMNS: usize = 4;

/// Size of every AES input and output block in bytes.
///
/// FIPS 197 §3.1 specifies a 128-bit block. Using `[u8; BLOCK_LEN]` makes a partial or oversized
/// block unrepresentable at this layer.
pub(super) const BLOCK_LEN: usize = 16;

/// AES's internal four-by-four byte state.
///
/// The outer index is the standard's row `r`; the inner index is its column `c`. This is a
/// deliberate representation choice: `rows[r][c]` reads like `s[r, c]`. It must not be confused
/// with input byte order, which is column-major according to FIPS 197 equation 3.6.
///
/// The type remains private so the public API cannot bypass the exact-size block boundary or
/// couple callers to this explanatory storage layout.
pub(super) struct State {
    rows: [[u8; STATE_COLUMNS]; STATE_ROWS],
}

impl State {
    /// Copy one complete input block into the state using FIPS 197 equation 3.6.
    ///
    /// The loop nesting walks columns first because consecutive groups of four input bytes form
    /// columns, not rows. For example, bytes `0`, `1`, `2`, and `3` become `s[0,0]`, `s[1,0]`,
    /// `s[2,0]`, and `s[3,0]`; byte `4` begins the second column at `s[0,1]`.
    #[must_use]
    pub(super) fn from_block(input: &[u8; BLOCK_LEN]) -> Self {
        let mut rows = [[0_u8; STATE_COLUMNS]; STATE_ROWS];

        for column in 0..STATE_COLUMNS {
            for row in 0..STATE_ROWS {
                rows[row][column] = input[row + STATE_ROWS * column];
            }
        }

        Self { rows }
    }

    /// Copy the current state to one complete output block using FIPS 197 equation 3.7.
    ///
    /// Every output position is overwritten. The same `row + 4 * column` index used while
    /// loading makes the operation the exact inverse representation mapping.
    pub(super) fn write_block(&self, output: &mut [u8; BLOCK_LEN]) {
        for column in 0..STATE_COLUMNS {
            for row in 0..STATE_ROWS {
                output[row + STATE_ROWS * column] = self.rows[row][column];
            }
        }
    }

    /// Read state byte `s[row, column]` using FIPS 197 §3.4's coordinate order.
    ///
    /// This private-layer accessor keeps the underlying row-major Rust storage from leaking into
    /// a transformation's indexing equations. AES layers call it only with indices in `0..4`.
    #[must_use]
    pub(super) fn byte(&self, row: usize, column: usize) -> u8 {
        self.rows[row][column]
    }

    /// Replace state byte `s[row, column]` using FIPS 197 §3.4's coordinate order.
    ///
    /// AES transformations call this only with indices in `0..4`; the fixed-size arrays retain a
    /// bounds check if an internal indexing error is introduced.
    pub(super) fn set_byte(&mut self, row: usize, column: usize, value: u8) {
        self.rows[row][column] = value;
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.rows.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::{BLOCK_LEN, State};

    /// Published intermediate-state evidence from FIPS 197-upd1 Appendix B, `input`.
    ///
    /// Appendix B prints the input byte sequence and its corresponding four state rows. This
    /// checks equation 3.6 against values published by the controlling standard rather than only
    /// checking that our load and store operations agree with each other.
    #[test]
    fn maps_appendix_b_input_into_the_published_state_rows() {
        let input = [
            0x32, 0x43, 0xf6, 0xa8, 0x88, 0x5a, 0x30, 0x8d, 0x31, 0x31, 0x98, 0xa2, 0xe0, 0x37,
            0x07, 0x34,
        ];

        let state = State::from_block(&input);

        assert_eq!(
            state.rows,
            [
                [0x32, 0x88, 0x31, 0xe0],
                [0x43, 0x5a, 0x31, 0x37],
                [0xf6, 0x30, 0x98, 0x07],
                [0xa8, 0x8d, 0xa2, 0x34],
            ]
        );
    }

    /// Standard-derived evidence from FIPS 197-upd1 §3.4, equations 3.6 and 3.7.
    ///
    /// Distinct values at every byte position make a transposition or row-major interpretation
    /// observable. This is not a NIST-published vector; it is a direct application of the two
    /// cited equations to a locally chosen sequential block.
    #[test]
    fn block_to_state_to_block_preserves_every_position() {
        let input = core::array::from_fn(|index| {
            u8::try_from(index).expect("every AES block index fits in u8")
        });
        let state = State::from_block(&input);
        let mut output = [0xff_u8; BLOCK_LEN];

        state.write_block(&mut output);

        assert_eq!(output, input);
    }
}
