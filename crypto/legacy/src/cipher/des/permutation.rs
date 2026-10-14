//! One direct implementation of every DES numbered-bit permutation.

/// Select input bits in table order, numbering the input's most-significant bit as one.
pub(super) fn permute(input: u64, input_width: u8, table: &[u8]) -> u64 {
    let mut output = 0_u64;
    for &source_position in table {
        debug_assert!((1..=input_width).contains(&source_position));
        let source_shift = input_width - source_position;
        output = (output << 1) | ((input >> source_shift) & 1);
    }
    output
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::cipher::des::constants::{FINAL_PERMUTATION, INITIAL_PERMUTATION};

    #[test]
    fn classic_initial_permutation_intermediate_round_trips() {
        let input = 0x0123_4567_89ab_cdef;
        let permuted = permute(input, 64, &INITIAL_PERMUTATION);
        assert_eq!(permuted, 0xcc00_ccff_f0aa_f0aa);
        assert_eq!(permute(permuted, 64, &FINAL_PERMUTATION), input);
    }
}
