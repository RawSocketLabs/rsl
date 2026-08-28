//! SHA-3 and SHAKE256, taught from the KECCAK permutation to a sponge.
//!
//! # What SHA-3 is
//!
//! FIPS 202 defines the SHA-3 family from one permutation, KECCAK-p[1600, 24], and the sponge
//! construction: absorb the padded message into the 1600-bit state `r` bits at a time, permute
//! between blocks, then squeeze output `r` bits at a time. SHA3-256 fixes the output at 256
//! bits; SHAKE256 is an extendable-output function (XOF) whose output length the caller
//! chooses. Ed448 (RFC 8032) uses SHAKE256 for key expansion, nonces, and challenges, which is
//! why this family is here.
//!
//! # FIPS 202 notation in Rust
//!
//! | FIPS 202 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `A[x, y, z]` | [`keccak::State`] lanes `[x][y]` as `u64` | §3.1 state array; bit `z` is lane bit `z`. |
//! | `θ`, `ρ`, `π`, `χ`, `ι` | [`keccak::State::theta`] … [`keccak::State::iota`] | §3.2 step mappings, each in place. |
//! | `Rnd`, KECCAK-p[1600, 24] | [`keccak::State::round`], [`keccak::State::permute`] | §3.3. |
//! | `RC[i_r]`, Table 2 | constants regenerated in tests from Algorithms 2 and 5 | §3.2.2, §3.2.5. |
//! | `pad10*1`, `r`, suffix `01` / `1111` | [`sponge::Sponge`] with `RATE` and suffix byte | §4, §5.1, §6. |
//! | SHA3-256(M) | [`Sha3_256`] | §6.1, `c = 512`, 256-bit output. |
//! | SHAKE256(M, d) | [`Shake256`] | §6.2, `c = 512`, `d`-bit output. |
//!
//! # Common mistakes and non-goals
//!
//! - The domain suffixes differ: `0x06` for SHA-3, `0x1f` for SHAKE. Mixing them silently gives
//!   a different, still-valid-looking output.
//! - Lanes are little-endian 64-bit words of the byte string; there is no big-endian step.
//! - SHA3-224/384/512, SHAKE128, cSHAKE, KMAC, and `TupleHash` are not provided.
//!
//! # Evidence and security status
//!
//! White-box tests reproduce NIST's printed state after each of `θ`, `ρ`, `π`, `χ`, `ι` in
//! round 0 and the final state for the 0-bit SHA3-256 example, and regenerate the round
//! constants and rotation offsets from their FIPS 202 algorithms. Public tests cover NIST's
//! 0-bit and 1600-bit SHA3-256 examples, the 0-bit SHAKE256 example, CAVP boundary lengths for
//! both, CAVP SHAKE256 variable-output cases, incremental squeezing, and differential comparison
//! with the `sha3` crate. No side-channel or audit claim is made; the permutation has no
//! secret-dependent branches or lookups.

#![allow(rustdoc::private_intra_doc_links)]

mod keccak;
mod sha3_256;
mod shake256;
mod sponge;

pub use sha3_256::{Sha3_256, Sha3_256Digest};
pub use shake256::Shake256;

/// Current project lifecycle classification for SHA3-256 and SHAKE256.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;

#[cfg(test)]
mod nist_intermediates {
    use super::keccak::State;

    fn decode(hex: &str) -> alloc::vec::Vec<u8> {
        (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }

    /// Published evidence: NIST `SHA3-256_Msg0.pdf`, round 0 state after each step mapping and
    /// the state after round 23 for the padded empty message.
    #[test]
    fn nist_sha3_256_empty_message_round_zero_step_mappings() {
        let fixtures = super::nist_fixtures::SHA3_256_MSG0;
        let mut padded = [0_u8; 136];
        padded[0] = 0x06;
        padded[135] = 0x80;
        let mut state = State::new();
        state.absorb_bytes(&padded);
        state.theta();
        assert_eq!(
            state.to_bytes().as_slice(),
            decode(fixtures.after_theta),
            "θ"
        );
        state.rho();
        assert_eq!(state.to_bytes().as_slice(), decode(fixtures.after_rho), "ρ");
        state.pi();
        assert_eq!(state.to_bytes().as_slice(), decode(fixtures.after_pi), "π");
        state.chi();
        assert_eq!(state.to_bytes().as_slice(), decode(fixtures.after_chi), "χ");
        state.iota(0);
        assert_eq!(
            state.to_bytes().as_slice(),
            decode(fixtures.after_iota),
            "ι"
        );
        for round in 1..24 {
            state.round(round);
        }
        assert_eq!(
            state.to_bytes().as_slice(),
            decode(fixtures.final_state),
            "round 23"
        );
    }
}

#[cfg(test)]
mod nist_fixtures;
