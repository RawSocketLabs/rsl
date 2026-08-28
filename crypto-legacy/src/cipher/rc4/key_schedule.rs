//! RC4 key-scheduling algorithm (KSA), named step by step.

/// Number of entries in RC4's byte permutation.
pub(super) const STATE_LEN: usize = 256;

/// Apply one KSA update and swap.
fn mix_step(state: &mut [u8; STATE_LEN], j: &mut u8, i: usize, key_byte: u8) {
    *j = j.wrapping_add(state[i]).wrapping_add(key_byte);
    state.swap(i, usize::from(*j));
}

/// Expand a validated non-empty key into RC4's initial permutation.
pub(super) fn schedule(key: &[u8]) -> [u8; STATE_LEN] {
    debug_assert!(!key.is_empty());
    let mut state = core::array::from_fn(|index| {
        u8::try_from(index).expect("RC4 state indices are smaller than 256")
    });
    let mut j = 0_u8;

    for i in 0..STATE_LEN {
        mix_step(&mut state, &mut j, i, key[i % key.len()]);
    }
    state
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn first_three_ksa_steps_make_the_named_swaps() {
        let mut state = core::array::from_fn(|index| u8::try_from(index).unwrap());
        let mut j = 0;

        mix_step(&mut state, &mut j, 0, 1);
        assert_eq!(j, 1);
        assert_eq!(&state[..4], &[1, 0, 2, 3]);

        mix_step(&mut state, &mut j, 1, 2);
        assert_eq!(j, 3);
        assert_eq!(&state[..4], &[1, 3, 2, 0]);

        mix_step(&mut state, &mut j, 2, 3);
        assert_eq!(j, 8);
        assert_eq!(state[2], 8);
        assert_eq!(state[8], 2);
    }

    #[test]
    fn completed_ksa_state_remains_a_permutation() {
        let state = schedule(&[1, 2, 3, 4, 5]);
        let mut seen = [false; STATE_LEN];
        for value in state {
            assert!(!seen[usize::from(value)]);
            seen[usize::from(value)] = true;
        }
        assert!(seen.into_iter().all(core::convert::identity));
    }
}
