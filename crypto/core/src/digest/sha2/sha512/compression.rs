//! SHA-512's eighty-round compression function.
//!
//! This module follows FIPS 180-4 §6.4.2 literally: initialize `a..h`, calculate `T1` and `T2`,
//! shift the working names, and feed the final values into the incoming chaining state.

use super::{
    constants::ROUND_CONSTANTS,
    functions::{big_sigma_0, big_sigma_1, choose, majority},
    schedule::{BLOCK_LEN, build_schedule},
};

/// The eight working variables named by FIPS 180-4 §6.4.2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorkingVariables {
    a: u64,
    b: u64,
    c: u64,
    d: u64,
    e: u64,
    f: u64,
    g: u64,
    h: u64,
}

impl WorkingVariables {
    /// Map `H_0..H_7` onto `a..h` without changing order.
    const fn from_chaining_value(value: [u64; 8]) -> Self {
        Self {
            a: value[0],
            b: value[1],
            c: value[2],
            d: value[3],
            e: value[4],
            f: value[5],
            g: value[6],
            h: value[7],
        }
    }
}

/// Perform one published SHA-512 compression round.
#[must_use]
fn perform_round(current: WorkingVariables, constant: u64, word: u64) -> WorkingVariables {
    let t1 = current
        .h
        .wrapping_add(big_sigma_1(current.e))
        .wrapping_add(choose(current.e, current.f, current.g))
        .wrapping_add(constant)
        .wrapping_add(word);
    let t2 = big_sigma_0(current.a).wrapping_add(majority(current.a, current.b, current.c));

    WorkingVariables {
        a: t1.wrapping_add(t2),
        b: current.a,
        c: current.b,
        d: current.c,
        e: current.d.wrapping_add(t1),
        f: current.e,
        g: current.f,
        h: current.g,
    }
}

/// Compress one 128-byte block into the supplied chaining value.
#[must_use]
pub(in crate::digest::sha2) fn compress_block(
    chaining_value: [u64; 8],
    block: &[u8; BLOCK_LEN],
) -> [u64; 8] {
    let schedule = build_schedule(block);
    let mut working = WorkingVariables::from_chaining_value(chaining_value);

    for round in 0..80 {
        working = perform_round(working, ROUND_CONSTANTS[round], schedule[round]);
    }

    [
        chaining_value[0].wrapping_add(working.a),
        chaining_value[1].wrapping_add(working.b),
        chaining_value[2].wrapping_add(working.c),
        chaining_value[3].wrapping_add(working.d),
        chaining_value[4].wrapping_add(working.e),
        chaining_value[5].wrapping_add(working.f),
        chaining_value[6].wrapping_add(working.g),
        chaining_value[7].wrapping_add(working.h),
    ]
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::digest::sha2::sha512::constants::INITIAL_HASH_VALUE;

    /// Standard-derived evidence for the exact first FIPS round over padded `abc`.
    #[test]
    fn first_abc_round_preserves_named_transition() {
        let mut block = [0_u8; BLOCK_LEN];
        block[..4].copy_from_slice(&[b'a', b'b', b'c', 0x80]);
        block[127] = 24;
        let schedule = build_schedule(&block);
        let before = WorkingVariables::from_chaining_value(INITIAL_HASH_VALUE);
        let after = perform_round(before, ROUND_CONSTANTS[0], schedule[0]);
        assert_eq!(after.b, before.a);
        assert_eq!(after.c, before.b);
        assert_eq!(after.d, before.c);
        assert_eq!(after.f, before.e);
        assert_eq!(after.g, before.f);
        assert_eq!(after.h, before.g);
        assert_eq!(after.a, 0xf6af_ceb8_bcfc_ddf5);
        assert_eq!(after.e, 0x58cb_0234_7ab5_1f91);
    }
}
