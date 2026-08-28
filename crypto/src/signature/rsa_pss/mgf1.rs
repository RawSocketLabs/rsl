//! RFC 8017 Appendix B.2.1 MGF1 mask generation with SHA-256.
//!
//! `MGF1(mgfSeed, maskLen)` concatenates `Hash(mgfSeed || C)` for a four-byte big-endian counter
//! `C = 0, 1, …` until at least `maskLen` bytes exist, then keeps the leading `maskLen` bytes.

use alloc::vec::Vec;

use crate::{Result, digest::sha2::sha256::Sha256};

/// Bytes produced by one SHA-256 block of mask output.
const HASH_LEN: usize = 32;

/// Appendix B.2.1 steps 1–4 for SHA-256.
///
/// # Errors
///
/// Propagates SHA-256 length errors only; a seed of a few dozen bytes cannot trigger them.
pub(super) fn mgf1_sha256(seed: &[u8], mask_len: usize) -> Result<Vec<u8>> {
    let mut mask = Vec::with_capacity(mask_len.div_ceil(HASH_LEN) * HASH_LEN);
    // Step 3: for counter from 0 to ceil(maskLen / hLen) - 1.
    for counter in 0..u32::try_from(mask_len.div_ceil(HASH_LEN)).unwrap_or(u32::MAX) {
        // 3.A: C = I2OSP(counter, 4).  3.B: T = T || Hash(mgfSeed || C).
        let mut hash = Sha256::new();
        hash.update(seed)?;
        hash.update(counter.to_be_bytes())?;
        mask.extend_from_slice(hash.finalize().as_bytes());
    }
    // Step 4: output the leading maskLen octets of T.
    mask.truncate(mask_len);
    Ok(mask)
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::digest::sha2::sha256::Sha256;

    /// Standard-derived evidence: the first block is `Hash(seed || 00000000)`, the second is
    /// `Hash(seed || 00000001)`, and the output is truncated exactly.
    #[test]
    fn mask_blocks_are_counter_suffixed_hashes_truncated_to_the_requested_length() {
        let seed = b"seed";
        let mask = mgf1_sha256(seed, 40).unwrap();
        let mut first = Sha256::new();
        first.update(seed).unwrap();
        first.update([0, 0, 0, 0]).unwrap();
        let mut second = Sha256::new();
        second.update(seed).unwrap();
        second.update([0, 0, 0, 1]).unwrap();
        assert_eq!(&mask[..32], first.finalize().as_bytes());
        assert_eq!(&mask[32..], &second.finalize().as_bytes()[..8]);
        assert!(mgf1_sha256(seed, 0).unwrap().is_empty());
    }
}
