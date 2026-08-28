//! SHA-1 schedule and compression from FIPS 180-4 §6.1.2.

/// Bytes in SHA-1's 512-bit block.
pub(super) const BLOCK_LEN: usize = 64;

/// Initial five-word chaining value from FIPS 180-4 §5.3.1.
pub(super) const INITIAL_STATE: [u32; 5] = [
    0x6745_2301,
    0xefcd_ab89,
    0x98ba_dcfe,
    0x1032_5476,
    0xc3d2_e1f0,
];

/// Parse and expand `W_0..W_79` exactly as §6.1.2 step 1 specifies.
fn schedule(block: &[u8; BLOCK_LEN]) -> [u32; 80] {
    let mut words = [0_u32; 80];
    for (index, word) in words[..16].iter_mut().enumerate() {
        let start = index * 4;
        *word = u32::from_be_bytes(
            block[start..start + 4]
                .try_into()
                .expect("word is four bytes"),
        );
    }
    for t in 16..80 {
        words[t] = (words[t - 3] ^ words[t - 8] ^ words[t - 14] ^ words[t - 16]).rotate_left(1);
    }
    words
}

/// Phase-specific Boolean function and additive constant from §§4.1.1 and 4.2.1.
const fn round_values(t: usize, b: u32, c: u32, d: u32) -> (u32, u32) {
    match t {
        0..=19 => ((b & c) ^ (!b & d), 0x5a82_7999),
        20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
        40..=59 => ((b & c) ^ (b & d) ^ (c & d), 0x8f1b_bcdc),
        _ => (b ^ c ^ d, 0xca62_c1d6),
    }
}

/// Apply eighty rounds and feed the result into one chaining value.
#[allow(clippy::many_single_char_names)] // `a` through `e` and `T` are the FIPS working names.
pub(super) fn compress(state: [u32; 5], block: &[u8; BLOCK_LEN]) -> [u32; 5] {
    let words = schedule(block);
    let [mut a, mut b, mut c, mut d, mut e] = state;
    for (t, word) in words.iter().copied().enumerate() {
        let (function, constant) = round_values(t, b, c, d);
        let temporary = a
            .rotate_left(5)
            .wrapping_add(function)
            .wrapping_add(e)
            .wrapping_add(constant)
            .wrapping_add(word);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temporary;
    }
    [
        state[0].wrapping_add(a),
        state[1].wrapping_add(b),
        state[2].wrapping_add(c),
        state[3].wrapping_add(d),
        state[4].wrapping_add(e),
    ]
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn padded_abc_schedule_preserves_parsing_and_first_expansion() {
        let mut block = [0_u8; 64];
        block[..4].copy_from_slice(&[b'a', b'b', b'c', 0x80]);
        block[63] = 24;
        let words = schedule(&block);
        assert_eq!(words[0], 0x6162_6380);
        assert_eq!(words[15], 24);
        assert_eq!(words[16], 0xc2c4_c700);
    }

    #[test]
    fn phase_boundaries_select_the_published_functions_and_constants() {
        let b = 0xaaaa_aaaa;
        let c = 0xcccc_cccc;
        let d = 0xf0f0_f0f0;
        assert_eq!(round_values(0, b, c, d).1, 0x5a82_7999);
        assert_eq!(round_values(20, b, c, d).1, 0x6ed9_eba1);
        assert_eq!(round_values(40, b, c, d).1, 0x8f1b_bcdc);
        assert_eq!(round_values(60, b, c, d).1, 0xca62_c1d6);
    }
}
