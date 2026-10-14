//! SHA-256 compression rounds and feed-forward.
//!
//! ## Implementation status
//!
//! This module implements initialization of the eight working variables, one independently
//! inspectable compression round, the loop that applies all 64 rounds, the final feed-forward
//! into the chaining value, and their composition into a complete one-block operation. Message
//! padding and iteration over multiple blocks remain state-layer responsibilities.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 180-4 §6.2.2][fips-180-4] defines this layer. Step 1 initializes working variables
//! `a` through `h` from the current eight-word chaining value. Step 2 performs rounds `t = 0`
//! through `63`, calculating `T_1` and `T_2` from the schedule word, `K_t`, and the six elementary
//! functions in §4.1.2. Step 3 adds the final working variables back into the chaining value.
//!
//! The implementation preserves all eight working variables and both temporary values as named
//! operations. Calculating `T_1` and `T_2` is separate from advancing the working variables, so
//! tests can identify precisely which half of a round failed. Post-round feed-forward remains a
//! separate operation with its own evidence. Parsing and schedule expansion belong to `schedule`;
//! message padding, block iteration, and final digest serialization belong to `state`.
//! NIST's [SHA-256 intermediate-value example][nist-sha256-example] supplies the published
//! working-variable states used to validate round zero and round 63 of the one-block `abc`
//! sample.
//!
//! [fips-180-4]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf
//! [nist-sha256-example]: https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/SHA256.pdf

use super::{
    constants::ROUND_CONSTANTS,
    functions::{big_sigma_0, big_sigma_1, choose, majority},
    schedule::{BLOCK_LEN, SCHEDULE_WORDS, build_schedule},
};

/// The eight SHA-256 working variables at one compression-round boundary.
///
/// FIPS 180-4 §6.2.2 names these variables `a` through `h`. A dedicated structure retains those
/// names instead of representing the round state as anonymous array indices. The values are one
/// round's local working state, not yet the eight-word chaining value stored between blocks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorkingVariables {
    /// Working variable `a`.
    a: u32,
    /// Working variable `b`.
    b: u32,
    /// Working variable `c`.
    c: u32,
    /// Working variable `d`.
    d: u32,
    /// Working variable `e`.
    e: u32,
    /// Working variable `f`.
    f: u32,
    /// Working variable `g`.
    g: u32,
    /// Working variable `h`.
    h: u32,
}

impl WorkingVariables {
    /// Initialize `a` through `h` from a chaining value in published word order.
    ///
    /// This is step 1 of FIPS 180-4 §6.2.2. Array index zero becomes `a`, index one becomes `b`,
    /// and so on through index seven becoming `h`.
    #[must_use]
    const fn from_chaining_value(chaining_value: [u32; 8]) -> Self {
        Self {
            a: chaining_value[0],
            b: chaining_value[1],
            c: chaining_value[2],
            d: chaining_value[3],
            e: chaining_value[4],
            f: chaining_value[5],
            g: chaining_value[6],
            h: chaining_value[7],
        }
    }
}

/// The two temporary words calculated during one SHA-256 compression round.
///
/// FIPS 180-4 §6.2.2 step 2 names these values `T_1` and `T_2`. They remain available after the
/// calculation so tests and future diagnostic tooling can inspect the equation results before
/// the working variables move to their next positions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RoundTemporaries {
    /// The `T_1` result for this round.
    t1: u32,
    /// The `T_2` result for this round.
    t2: u32,
}

/// The inspectable result of one SHA-256 compression round.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RoundComputation {
    /// The equation results calculated before the state transition.
    temporaries: RoundTemporaries,
    /// Working variables at the boundary immediately after this round.
    next: WorkingVariables,
}

/// Calculate `T_1` and `T_2` for one compression round.
///
/// **Standard mapping:** FIPS 180-4 §6.2.2 step 2 defines
/// `T_1 = h + Sigma1(e) + Ch(e, f, g) + K_t + W_t` and
/// `T_2 = Sigma0(a) + Maj(a, b, c)`.
///
/// **Rust mapping:** every `wrapping_add` is addition modulo `2^32` as required by FIPS 180-4
/// §3.2. `round_constant` represents `K_t`, and `schedule_word` represents `W_t` for the same
/// round index `t`.
#[must_use]
const fn calculate_temporaries(
    current: WorkingVariables,
    round_constant: u32,
    schedule_word: u32,
) -> RoundTemporaries {
    let t1 = current
        .h
        .wrapping_add(big_sigma_1(current.e))
        .wrapping_add(choose(current.e, current.f, current.g))
        .wrapping_add(round_constant)
        .wrapping_add(schedule_word);
    let t2 = big_sigma_0(current.a).wrapping_add(majority(current.a, current.b, current.c));

    RoundTemporaries { t1, t2 }
}

/// Advance `a` through `h` using one round's `T_1` and `T_2` values.
///
/// **Standard mapping:** FIPS 180-4 §6.2.2 step 2 assigns `a = T_1 + T_2` and
/// `e = d + T_1`. Each remaining output receives the previous value immediately to its left in
/// the published assignment list: `b = a`, `c = b`, `d = c`, `f = e`, `g = f`, and `h = g`.
///
/// The function accepts the temporaries separately so the state movement can be tested without
/// repeating their equations.
#[must_use]
const fn advance_working_variables(
    current: WorkingVariables,
    temporaries: RoundTemporaries,
) -> WorkingVariables {
    WorkingVariables {
        a: temporaries.t1.wrapping_add(temporaries.t2),
        b: current.a,
        c: current.b,
        d: current.c,
        e: current.d.wrapping_add(temporaries.t1),
        f: current.e,
        g: current.f,
        h: current.g,
    }
}

/// Perform one complete SHA-256 compression round.
///
/// This composes the two explicit halves of FIPS 180-4 §6.2.2 step 2: calculate `T_1` and `T_2`,
/// then advance the eight working variables. The caller is responsible for pairing the correct
/// `K_t` and `W_t`; [`run_rounds`] owns that shared ordering for a complete block.
#[must_use]
const fn perform_round(
    current: WorkingVariables,
    round_constant: u32,
    schedule_word: u32,
) -> RoundComputation {
    let temporaries = calculate_temporaries(current, round_constant, schedule_word);
    let next = advance_working_variables(current, temporaries);

    RoundComputation { temporaries, next }
}

/// Run all 64 compression rounds without applying feed-forward.
///
/// **Standard mapping:** this implements FIPS 180-4 §6.2.2 step 1 and the complete `t = 0`
/// through `63` loop in step 2. Step 3, which adds the resulting working variables back into the
/// input chaining value, is deliberately not performed here.
///
/// **Rust mapping:** `ROUND_CONSTANTS` and `schedule` are both fixed-size 64-word arrays in
/// standard order. Zipping their iterators pairs `K_t` and `W_t` without maintaining a second,
/// independently mutable index. Each round starts from the preceding round's `next` variables.
#[must_use]
fn run_rounds(chaining_value: [u32; 8], schedule: &[u32; SCHEDULE_WORDS]) -> WorkingVariables {
    let mut working = WorkingVariables::from_chaining_value(chaining_value);

    for (round_constant, schedule_word) in ROUND_CONSTANTS
        .iter()
        .copied()
        .zip(schedule.iter().copied())
    {
        working = perform_round(working, round_constant, schedule_word).next;
    }

    working
}

/// Add the round-63 working variables into the input chaining value.
///
/// **Standard mapping:** this is FIPS 180-4 §6.2.2 step 3. Each input hash word `H_i^(N-1)` is
/// added to exactly one corresponding working variable to produce `H_i^N`: `a` contributes to
/// word zero, `b` to word one, and so on through `h` contributing to word seven.
///
/// **Rust mapping:** every word uses `wrapping_add` because FIPS 180-4 §3.2 requires addition
/// modulo `2^32`. Keeping all eight assignments visible makes ordering errors reviewable.
#[must_use]
const fn feed_forward(chaining_value: [u32; 8], working: WorkingVariables) -> [u32; 8] {
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

/// Compress one complete 512-bit block into an eight-word chaining value.
///
/// **Standard mapping:** this composes FIPS 180-4 §6.2.2 steps 1 through 3 for one block: build
/// the 64-word schedule, initialize and run all 64 compression rounds, then feed the resulting
/// working variables into the block's input chaining value.
///
/// **Boundary:** the block type requires exactly 64 bytes. This function neither pads input nor
/// chooses the initial hash value; callers provide the appropriate chaining value so the same
/// operation works for both the first and later message blocks.
#[must_use]
pub(super) fn compress_block(chaining_value: [u32; 8], block: &[u8; BLOCK_LEN]) -> [u32; 8] {
    let schedule = build_schedule(block);
    let working = run_rounds(chaining_value, &schedule);

    feed_forward(chaining_value, working)
}

#[cfg(test)]
mod unit {
    use super::{
        RoundTemporaries, WorkingVariables, advance_working_variables, calculate_temporaries,
        compress_block, feed_forward, perform_round, run_rounds,
    };
    use crate::digest::sha2::sha256::{
        constants::{INITIAL_HASH_VALUE, ROUND_CONSTANTS},
        schedule::{BLOCK_LEN, build_schedule},
    };

    /// Published-example fixture from NIST's SHA-256 one-block `abc` sample.
    ///
    /// The bytes reproduce the sample's `W_0` through `W_15`. Padding is still supplied as test
    /// data rather than produced by a state-layer implementation.
    fn padded_abc_block() -> [u8; BLOCK_LEN] {
        let mut block = [0_u8; BLOCK_LEN];
        block[0] = b'a';
        block[1] = b'b';
        block[2] = b'c';
        block[3] = 0x80;
        block[63] = 24;
        block
    }

    /// Standard-derived evidence for FIPS 180-4 §6.2.2 step 1.
    #[test]
    fn chaining_words_initialize_working_variables_in_published_order() {
        let working = WorkingVariables::from_chaining_value(INITIAL_HASH_VALUE);

        assert_eq!(
            working,
            WorkingVariables {
                a: 0x6a09_e667,
                b: 0xbb67_ae85,
                c: 0x3c6e_f372,
                d: 0xa54f_f53a,
                e: 0x510e_527f,
                f: 0x9b05_688c,
                g: 0x1f83_d9ab,
                h: 0x5be0_cd19,
            }
        );
    }

    /// Standard-derived evidence from the FIPS 180-4 §6.2.2 `T_1` and `T_2` equations.
    ///
    /// NIST publishes the round input and resulting working variables, but not these two
    /// temporary values. They are therefore derived expectations rather than published vectors.
    #[test]
    fn first_abc_round_calculates_the_expected_temporaries() {
        let schedule = build_schedule(&padded_abc_block());
        let current = WorkingVariables::from_chaining_value(INITIAL_HASH_VALUE);

        let temporaries = calculate_temporaries(current, ROUND_CONSTANTS[0], schedule[0]);

        assert_eq!(temporaries.t1, 0x54da_50e8, "T_1");
        assert_eq!(temporaries.t2, 0x0890_9ae5, "T_2");
    }

    /// Published intermediate-state evidence from NIST's SHA-256 one-block `abc` sample, `t=0`.
    #[test]
    fn first_abc_round_matches_nists_published_working_variables() {
        let schedule = build_schedule(&padded_abc_block());
        let current = WorkingVariables::from_chaining_value(INITIAL_HASH_VALUE);

        let computation = perform_round(current, ROUND_CONSTANTS[0], schedule[0]);

        assert_eq!(computation.temporaries.t1, 0x54da_50e8, "T_1");
        assert_eq!(computation.temporaries.t2, 0x0890_9ae5, "T_2");
        assert_eq!(
            computation.next,
            WorkingVariables {
                a: 0x5d6a_ebcd,
                b: 0x6a09_e667,
                c: 0xbb67_ae85,
                d: 0x3c6e_f372,
                e: 0xfa2a_4622,
                f: 0x510e_527f,
                g: 0x9b05_688c,
                h: 0x1f83_d9ab,
            }
        );
    }

    /// Published intermediate-state evidence from NIST's SHA-256 one-block `abc` sample, `t=63`.
    ///
    /// Matching the final round boundary verifies that the driver applies every `K_t` and `W_t`
    /// pair in order. These are the working variables before FIPS 180-4 §6.2.2 step 3
    /// feed-forward, not the final digest words.
    #[test]
    fn all_64_abc_rounds_reach_nists_published_t63_working_variables() {
        let schedule = build_schedule(&padded_abc_block());

        let working = run_rounds(INITIAL_HASH_VALUE, &schedule);

        assert_eq!(
            working,
            WorkingVariables {
                a: 0x506e_3058,
                b: 0xd39a_2165,
                c: 0x04d2_4d6c,
                d: 0xb85e_2ce9,
                e: 0x5ef5_0f24,
                f: 0xfb12_1210,
                g: 0x948d_25b6,
                h: 0x961f_4894,
            }
        );
    }

    /// Published output-state evidence from NIST's SHA-256 one-block `abc` sample.
    ///
    /// NIST prints each initial word, round-63 working variable, modular addition, and resulting
    /// hash word. This test isolates FIPS 180-4 §6.2.2 step 3 from schedule and round execution.
    #[test]
    fn feed_forward_matches_nists_published_abc_hash_words() {
        let round_63 = WorkingVariables {
            a: 0x506e_3058,
            b: 0xd39a_2165,
            c: 0x04d2_4d6c,
            d: 0xb85e_2ce9,
            e: 0x5ef5_0f24,
            f: 0xfb12_1210,
            g: 0x948d_25b6,
            h: 0x961f_4894,
        };

        let output = feed_forward(INITIAL_HASH_VALUE, round_63);

        assert_eq!(
            output,
            [
                0xba78_16bf,
                0x8f01_cfea,
                0x4141_40de,
                0x5dae_2223,
                0xb003_61a3,
                0x9617_7a9c,
                0xb410_ff61,
                0xf200_15ad,
            ]
        );
    }

    /// Standard-derived boundary evidence for the modulo-`2^32` additions in FIPS 180-4 §3.2
    /// and §6.2.2 step 3.
    #[test]
    fn feed_forward_wraps_each_hash_word_independently() {
        let working = WorkingVariables {
            a: u32::MAX,
            b: u32::MAX,
            c: u32::MAX,
            d: u32::MAX,
            e: u32::MAX,
            f: u32::MAX,
            g: u32::MAX,
            h: u32::MAX,
        };

        assert_eq!(feed_forward([1; 8], working), [0; 8]);
    }

    /// Published known-answer evidence from NIST's SHA-256 one-block `abc` sample.
    ///
    /// This test crosses the parsing, schedule, round-loop, and feed-forward boundaries while
    /// still supplying already padded input. Padding and digest-byte serialization are outside
    /// the operation under test.
    #[test]
    fn complete_block_compression_matches_nists_published_abc_hash_words() {
        let output = compress_block(INITIAL_HASH_VALUE, &padded_abc_block());

        assert_eq!(
            output,
            [
                0xba78_16bf,
                0x8f01_cfea,
                0x4141_40de,
                0x5dae_2223,
                0xb003_61a3,
                0x9617_7a9c,
                0xb410_ff61,
                0xf200_15ad,
            ]
        );
    }

    /// Standard-derived boundary evidence for modulo-`2^32` additions in FIPS 180-4 §3.2 and
    /// §6.2.2 step 2.
    #[test]
    fn temporary_and_transition_additions_wrap_at_32_bits() {
        let zero = WorkingVariables {
            a: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            f: 0,
            g: 0,
            h: 0,
        };
        let temporaries = calculate_temporaries(zero, u32::MAX, 1);

        assert_eq!(temporaries, RoundTemporaries { t1: 0, t2: 0 });

        let current = WorkingVariables {
            a: 1,
            b: 2,
            c: 3,
            d: 1,
            e: 5,
            f: 6,
            g: 7,
            h: 8,
        };
        let next = advance_working_variables(
            current,
            RoundTemporaries {
                t1: u32::MAX,
                t2: 1,
            },
        );

        assert_eq!(
            next,
            WorkingVariables {
                a: 0,
                b: 1,
                c: 2,
                d: 3,
                e: 0,
                f: 5,
                g: 6,
                h: 7,
            }
        );
    }
}
