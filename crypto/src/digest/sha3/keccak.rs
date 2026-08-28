//! FIPS 202 §3: the KECCAK-p[1600, 24] permutation, one step mapping at a time.
//!
//! ## Standards ownership
//!
//! §3.1 defines the state as a 5×5 array of 64-bit lanes `A[x, y]`, with lane `(x, y)` occupying
//! bytes `8·(5y + x)` onward of the 200-byte string and each lane read little-endian (§3.1.2,
//! Algorithm 10 / `b2h`). §3.2 defines the five step mappings `θ`, `ρ`, `π`, `χ`, `ι`; §3.3 the
//! round `Rnd = ι ∘ χ ∘ π ∘ ρ ∘ θ` and the 24-round permutation. Each mapping below is a separate
//! function that mutates the state in place so NIST's printed per-step intermediates can be
//! checked one at a time.

use zeroize::Zeroize;

/// Lanes per row and per column (§3.1: `5 × 5 × w`).
const SIDE: usize = 5;
/// Rounds of KECCAK-p[1600, 24] (§3.4, `n_r = 12 + 2ℓ` with `ℓ = 6`).
pub(super) const ROUNDS: usize = 24;
/// Bytes in the 1600-bit state.
pub(super) const STATE_BYTES: usize = 200;

/// §3.2.2, Table 2: rotation offsets of `ρ`, indexed `[x][y]`.
///
/// Table 2 prints columns in the order `x = 3, 4, 0, 1, 2`; this array is in natural `x` order,
/// and the unit test recomputes every entry from Algorithm 2 step 3a.
const RHO_OFFSETS: [[u32; SIDE]; SIDE] = [
    [0, 36, 3, 41, 18],
    [1, 44, 10, 45, 2],
    [62, 6, 43, 15, 61],
    [28, 55, 25, 21, 56],
    [27, 20, 39, 8, 14],
];

/// §3.2.5: the round constants `RC[i_r]` for `ℓ = 6`, produced by Algorithm 5's LFSR.
///
/// The unit test regenerates all 24 from `rc(t)` so the table cannot drift from the definition.
const ROUND_CONSTANTS: [u64; ROUNDS] = [
    0x0000_0000_0000_0001,
    0x0000_0000_0000_8082,
    0x8000_0000_0000_808a,
    0x8000_0000_8000_8000,
    0x0000_0000_0000_808b,
    0x0000_0000_8000_0001,
    0x8000_0000_8000_8081,
    0x8000_0000_0000_8009,
    0x0000_0000_0000_008a,
    0x0000_0000_0000_0088,
    0x0000_0000_8000_8009,
    0x0000_0000_8000_000a,
    0x0000_0000_8000_808b,
    0x8000_0000_0000_008b,
    0x8000_0000_0000_8089,
    0x8000_0000_0000_8003,
    0x8000_0000_0000_8002,
    0x8000_0000_0000_0080,
    0x0000_0000_0000_800a,
    0x8000_0000_8000_000a,
    0x8000_0000_8000_8081,
    0x8000_0000_0000_8080,
    0x0000_0000_8000_0001,
    0x8000_0000_8000_8008,
];

/// The 5×5 lane state `A[x][y]`.
///
/// It is zeroized on drop because a sponge state carries key-derived material in Ed448.
pub(super) struct State {
    lanes: [[u64; SIDE]; SIDE],
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            lanes: [[0; SIDE]; SIDE],
        }
    }

    /// §3.1.2: XOR a byte string of at most 200 bytes into the state, lane by lane.
    pub(super) fn absorb_bytes(&mut self, bytes: &[u8]) {
        debug_assert!(bytes.len() <= STATE_BYTES);
        for (index, byte) in bytes.iter().enumerate() {
            let lane = index / 8;
            let (x, y) = (lane % SIDE, lane / SIDE);
            self.lanes[x][y] ^= u64::from(*byte) << (8 * (index % 8));
        }
    }

    /// §3.1.3: read the leading `out.len()` bytes of the state string.
    pub(super) fn squeeze_bytes(&self, out: &mut [u8]) {
        debug_assert!(out.len() <= STATE_BYTES);
        for (index, byte) in out.iter_mut().enumerate() {
            let lane = index / 8;
            let (x, y) = (lane % SIDE, lane / SIDE);
            *byte = self.lanes[x][y].to_le_bytes()[index % 8];
        }
    }

    /// §3.2.1 Algorithm 1: `θ`, XOR each lane with the parities of two neighbouring columns.
    pub(super) fn theta(&mut self) {
        let mut column_parity = [0_u64; SIDE];
        for (x, parity) in column_parity.iter_mut().enumerate() {
            *parity = self.lanes[x].iter().fold(0, |acc, lane| acc ^ lane);
        }
        for x in 0..SIDE {
            let d = column_parity[(x + 4) % SIDE] ^ column_parity[(x + 1) % SIDE].rotate_left(1);
            for lane in &mut self.lanes[x] {
                *lane ^= d;
            }
        }
    }

    /// §3.2.2 Algorithm 2: `ρ`, rotate each lane by its Table 2 offset.
    pub(super) fn rho(&mut self) {
        for (column, offsets) in self.lanes.iter_mut().zip(RHO_OFFSETS.iter()) {
            for (lane, offset) in column.iter_mut().zip(offsets.iter()) {
                *lane = lane.rotate_left(*offset);
            }
        }
    }

    /// §3.2.3 Algorithm 3: `π`, `A′[x, y] = A[(x + 3y) mod 5, x]`.
    pub(super) fn pi(&mut self) {
        let previous = self.lanes;
        for x in 0..SIDE {
            for y in 0..SIDE {
                self.lanes[x][y] = previous[(x + 3 * y) % SIDE][x];
            }
        }
    }

    /// §3.2.4 Algorithm 4: `χ`, `A′[x, y] = A[x, y] ⊕ (¬A[x+1, y] ∧ A[x+2, y])`.
    pub(super) fn chi(&mut self) {
        let previous = self.lanes;
        for (x, column) in self.lanes.iter_mut().enumerate() {
            for (y, lane) in column.iter_mut().enumerate() {
                *lane =
                    previous[x][y] ^ (!previous[(x + 1) % SIDE][y] & previous[(x + 2) % SIDE][y]);
            }
        }
    }

    /// §3.2.5 Algorithm 6: `ι`, XOR the round constant into lane `(0, 0)`.
    pub(super) fn iota(&mut self, round: usize) {
        self.lanes[0][0] ^= ROUND_CONSTANTS[round];
    }

    /// §3.3: one round `Rnd(A, i_r) = ι(χ(π(ρ(θ(A)))), i_r)`.
    pub(super) fn round(&mut self, round: usize) {
        self.theta();
        self.rho();
        self.pi();
        self.chi();
        self.iota(round);
    }

    /// §3.3 Algorithm 7: KECCAK-p[1600, 24], rounds `0..24`.
    pub(super) fn permute(&mut self) {
        for round in 0..ROUNDS {
            self.round(round);
        }
    }

    /// The complete 200-byte state string, for published intermediate checks.
    #[cfg(test)]
    pub(super) fn to_bytes(&self) -> [u8; STATE_BYTES] {
        let mut out = [0_u8; STATE_BYTES];
        self.squeeze_bytes(&mut out);
        out
    }
}

impl Drop for State {
    fn drop(&mut self) {
        for column in &mut self.lanes {
            column.zeroize();
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    /// FIPS 202 Algorithm 5: `rc(t)` from the LFSR `x^8 + x^6 + x^5 + x^4 + 1`.
    ///
    /// FIPS 202 writes bit strings least-significant-first: `R = 10000000` is the integer 1, the
    /// shift `0 || R` doubles it, and `R[0]` is the low bit.
    fn rc(t: usize) -> bool {
        let mut r: u16 = 1;
        for _ in 1..=(t % 255) {
            r <<= 1;
            if r & 0x100 != 0 {
                r ^= 0x71;
            }
            r &= 0xff;
        }
        r & 1 == 1
    }

    /// Standard-derived evidence: every `RC[i_r]` regenerated from Algorithm 5 / Algorithm 6.
    #[test]
    fn round_constants_match_the_algorithm_5_lfsr() {
        for (round, expected) in ROUND_CONSTANTS.iter().enumerate() {
            let mut constant = 0_u64;
            for j in 0..=6 {
                if rc(j + 7 * round) {
                    constant |= 1 << ((1 << j) - 1);
                }
            }
            assert_eq!(constant, *expected, "RC[{round}]");
        }
    }

    /// Standard-derived evidence: Table 2 regenerated from Algorithm 2 step 3a.
    #[test]
    fn rho_offsets_match_algorithm_2() {
        let mut offsets = [[0_u32; SIDE]; SIDE];
        let (mut x, mut y) = (1, 0);
        for t in 0..24_u32 {
            offsets[x][y] = ((t + 1) * (t + 2) / 2) % 64;
            (x, y) = (y, (2 * x + 3 * y) % SIDE);
        }
        assert_eq!(offsets, RHO_OFFSETS);
    }

    /// Standard-derived evidence: byte/lane mapping round-trips in §3.1.2 order.
    #[test]
    fn byte_lane_mapping_round_trips() {
        let bytes: [u8; STATE_BYTES] = core::array::from_fn(|i| u8::try_from(i).unwrap());
        let mut state = State::new();
        state.absorb_bytes(&bytes);
        assert_eq!(state.lanes[0][0], 0x0706_0504_0302_0100);
        assert_eq!(state.lanes[1][0], 0x0f0e_0d0c_0b0a_0908);
        assert_eq!(state.lanes[0][1], 0x2f2e_2d2c_2b2a_2928);
        assert_eq!(state.to_bytes(), bytes);
    }
}
