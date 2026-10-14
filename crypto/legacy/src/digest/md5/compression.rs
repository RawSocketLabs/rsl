//! RFC 1321 §3.4's four-round, 64-step MD5 compression operation.

pub(super) const BLOCK_LEN: usize = 64;
pub(super) const INITIAL_STATE: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];

/// Per-step left-rotation distances from RFC 1321 §3.4.
const SHIFTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// RFC 1321's `T[i] = floor(2^32 * abs(sin(i)))`, for one-based `i=1..64`.
const ADDITIVE_CONSTANTS: [u32; 64] = [
    0xd76a_a478,
    0xe8c7_b756,
    0x2420_70db,
    0xc1bd_ceee,
    0xf57c_0faf,
    0x4787_c62a,
    0xa830_4613,
    0xfd46_9501,
    0x6980_98d8,
    0x8b44_f7af,
    0xffff_5bb1,
    0x895c_d7be,
    0x6b90_1122,
    0xfd98_7193,
    0xa679_438e,
    0x49b4_0821,
    0xf61e_2562,
    0xc040_b340,
    0x265e_5a51,
    0xe9b6_c7aa,
    0xd62f_105d,
    0x0244_1453,
    0xd8a1_e681,
    0xe7d3_fbc8,
    0x21e1_cde6,
    0xc337_07d6,
    0xf4d5_0d87,
    0x455a_14ed,
    0xa9e3_e905,
    0xfcef_a3f8,
    0x676f_02d9,
    0x8d2a_4c8a,
    0xfffa_3942,
    0x8771_f681,
    0x6d9d_6122,
    0xfde5_380c,
    0xa4be_ea44,
    0x4bde_cfa9,
    0xf6bb_4b60,
    0xbebf_bc70,
    0x289b_7ec6,
    0xeaa1_27fa,
    0xd4ef_3085,
    0x0488_1d05,
    0xd9d4_d039,
    0xe6db_99e5,
    0x1fa2_7cf8,
    0xc4ac_5665,
    0xf429_2244,
    0x432a_ff97,
    0xab94_23a7,
    0xfc93_a039,
    0x655b_59c3,
    0x8f0c_cc92,
    0xffef_f47d,
    0x8584_5dd1,
    0x6fa8_7e4f,
    0xfe2c_e6e0,
    0xa301_4314,
    0x4e08_11a1,
    0xf753_7e82,
    0xbd3a_f235,
    0x2ad7_d2bb,
    0xeb86_d391,
];

/// Select the round Boolean function and message word index for step `i`.
const fn step_values(i: usize, b: u32, c: u32, d: u32) -> (u32, usize) {
    match i {
        0..=15 => ((b & c) | (!b & d), i),
        16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
        32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
        _ => (c ^ (b | !d), (7 * i) % 16),
    }
}

fn words(block: &[u8; BLOCK_LEN]) -> [u32; 16] {
    core::array::from_fn(|index| {
        let start = index * 4;
        u32::from_le_bytes(
            block[start..start + 4]
                .try_into()
                .expect("word is four bytes"),
        )
    })
}

/// Apply all four rounds and feed the working words into the incoming state.
#[allow(clippy::many_single_char_names)] // `A` through `D` preserve RFC 1321's notation.
pub(super) fn compress(state: [u32; 4], block: &[u8; BLOCK_LEN]) -> [u32; 4] {
    let message = words(block);
    let [mut a, mut b, mut c, mut d] = state;
    for i in 0..64 {
        let (function, message_index) = step_values(i, b, c, d);
        let next_b = b.wrapping_add(
            a.wrapping_add(function)
                .wrapping_add(ADDITIVE_CONSTANTS[i])
                .wrapping_add(message[message_index])
                .rotate_left(SHIFTS[i]),
        );
        a = d;
        d = c;
        c = b;
        b = next_b;
    }
    [
        state[0].wrapping_add(a),
        state[1].wrapping_add(b),
        state[2].wrapping_add(c),
        state[3].wrapping_add(d),
    ]
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn parsing_is_little_endian_and_round_indices_follow_all_four_permutations() {
        let block = core::array::from_fn(|index| u8::try_from(index).expect("block index fits u8"));
        assert_eq!(words(&block)[0], 0x0302_0100);
        assert_eq!(words(&block)[15], 0x3f3e_3d3c);
        assert_eq!(step_values(0, 0, 0, 0).1, 0);
        assert_eq!(step_values(16, 0, 0, 0).1, 1);
        assert_eq!(step_values(32, 0, 0, 0).1, 5);
        assert_eq!(step_values(48, 0, 0, 0).1, 0);
    }

    #[test]
    fn constants_cover_the_first_and_last_step_exactly() {
        assert_eq!(ADDITIVE_CONSTANTS[0], 0xd76a_a478);
        assert_eq!(ADDITIVE_CONSTANTS[63], 0xeb86_d391);
        assert_eq!(SHIFTS[0], 7);
        assert_eq!(SHIFTS[63], 21);
    }
}
