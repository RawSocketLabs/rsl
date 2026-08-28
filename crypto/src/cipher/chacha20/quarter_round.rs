//! RFC 8439 §2.1 quarter round and §2.2 quarter round on the `ChaCha` state.
//!
//! ## Standards ownership
//!
//! §2.1 defines `QUARTERROUND(a, b, c, d)` as four add–xor–rotate triples on 32-bit words, in
//! the printed order, with rotations of 16, 12, 8, and 7 bits. §2.2 applies it to four positions
//! of the sixteen-word state. Both are written out literally here.

/// Number of 32-bit words in a `ChaCha` state.
pub(super) const STATE_WORDS: usize = 16;

/// §2.1: one quarter round on four words, in the printed operation order.
///
/// ```text
/// 1.  a += b; d ^= a; d <<<= 16;
/// 2.  c += d; b ^= c; b <<<= 12;
/// 3.  a += b; d ^= a; d <<<= 8;
/// 4.  c += d; b ^= c; b <<<= 7;
/// ```
///
/// `+=` is addition modulo `2^32` (`wrapping_add`), `^=` is XOR, and `<<<=` is a left rotation.
pub(super) fn quarter_round(a: &mut u32, b: &mut u32, c: &mut u32, d: &mut u32) {
    *a = a.wrapping_add(*b);
    *d ^= *a;
    *d = d.rotate_left(16);

    *c = c.wrapping_add(*d);
    *b ^= *c;
    *b = b.rotate_left(12);

    *a = a.wrapping_add(*b);
    *d ^= *a;
    *d = d.rotate_left(8);

    *c = c.wrapping_add(*d);
    *b ^= *c;
    *b = b.rotate_left(7);
}

/// §2.2: `QUARTERROUND(x, y, z, w)` applied to state positions `x`, `y`, `z`, and `w`.
#[allow(clippy::many_single_char_names)] // `a`–`d` and `x`–`w` are the RFC's names.
pub(super) fn quarter_round_on_state(
    state: &mut [u32; STATE_WORDS],
    x: usize,
    y: usize,
    z: usize,
    w: usize,
) {
    let (mut a, mut b, mut c, mut d) = (state[x], state[y], state[z], state[w]);
    quarter_round(&mut a, &mut b, &mut c, &mut d);
    state[x] = a;
    state[y] = b;
    state[z] = c;
    state[w] = d;
}

#[cfg(test)]
mod unit {
    use super::*;

    /// Published evidence: RFC 8439 §2.1.1 quarter-round test vector.
    #[test]
    fn rfc_8439_section_2_1_1_quarter_round() {
        let (mut a, mut b, mut c, mut d) = (0x1111_1111, 0x0102_0304, 0x9b8d_6f43, 0x0123_4567);
        quarter_round(&mut a, &mut b, &mut c, &mut d);
        assert_eq!(
            (a, b, c, d),
            (0xea2a_92f4, 0xcb1c_f8ce, 0x4581_472e, 0x5881_c4bb)
        );
    }

    /// Published evidence: RFC 8439 §2.2.1 `QUARTERROUND(2, 7, 8, 13)` on a sample state.
    #[test]
    fn rfc_8439_section_2_2_1_diagonal_round_changes_only_four_words() {
        let mut state = [
            0x8795_31e0,
            0xc5ec_f37d,
            0x5164_61b1,
            0xc9a6_2f8a,
            0x44c2_0ef3,
            0x3390_af7f,
            0xd9fc_690b,
            0x2a5f_714c,
            0x5337_2767,
            0xb00a_5631,
            0x974c_541a,
            0x359e_9963,
            0x5c97_1061,
            0x3d63_1689,
            0x2098_d9d6,
            0x91db_d320,
        ];
        let before = state;
        quarter_round_on_state(&mut state, 2, 7, 8, 13);
        let expected = [
            0x8795_31e0,
            0xc5ec_f37d,
            0xbdb8_86dc,
            0xc9a6_2f8a,
            0x44c2_0ef3,
            0x3390_af7f,
            0xd9fc_690b,
            0xcfac_afd2,
            0xe46b_ea80,
            0xb00a_5631,
            0x974c_541a,
            0x359e_9963,
            0x5c97_1061,
            0xccc0_7c79,
            0x2098_d9d6,
            0x91db_d320,
        ];
        assert_eq!(state, expected);
        for index in (0..16).filter(|index| ![2, 7, 8, 13].contains(index)) {
            assert_eq!(state[index], before[index], "word {index} unchanged");
        }
    }
}
