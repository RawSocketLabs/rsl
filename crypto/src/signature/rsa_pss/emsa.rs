//! RFC 8017 §9.1.2 EMSA-PSS-VERIFY with SHA-256 and MGF1-SHA-256.
//!
//! The encoded message has the layout
//!
//! ```text
//! EM = maskedDB || H || 0xbc
//! DB = PS (zero padding) || 0x01 || salt
//! H  = Hash(0x00 * 8 || mHash || salt)
//! ```
//!
//! and verification recomputes `H` from the recovered salt. Every step below carries the
//! standard's number. All checks are performed before the final comparison so the sequence is
//! the same for every malformed encoding.

use alloc::vec::Vec;
use zeroize::Zeroize;

use super::mgf1::mgf1_sha256;
use crate::{CryptoError, Result, digest::sha2::sha256::Sha256};

/// SHA-256 output length `hLen`.
pub(super) const HASH_LEN: usize = 32;

/// §9.1.2 with `Hash = SHA-256`, `MGF = MGF1-SHA-256`, and caller-selected `sLen`.
///
/// `em_bits` is `modBits - 1`; `encoded` is `EM` of length `ceil(em_bits / 8)`.
///
/// # Errors
///
/// Returns [`CryptoError::InvalidSignature`] for any of the standard's "inconsistent" outcomes.
pub(super) fn emsa_pss_verify_sha256(
    message_hash: &[u8; HASH_LEN],
    encoded: &[u8],
    em_bits: usize,
    salt_len: usize,
) -> Result<()> {
    let em_len = em_bits.div_ceil(8);
    if encoded.len() != em_len {
        return Err(CryptoError::InvalidSignature);
    }

    // Step 3: emLen < hLen + sLen + 2 is inconsistent.
    if em_len < HASH_LEN + salt_len + 2 {
        return Err(CryptoError::InvalidSignature);
    }

    // Step 4: the rightmost octet must be 0xbc.
    let mut consistent = encoded[em_len - 1] == 0xbc;

    // Step 5: split EM into maskedDB (emLen - hLen - 1 octets) and H (hLen octets).
    let db_len = em_len - HASH_LEN - 1;
    let masked_db = &encoded[..db_len];
    let h = &encoded[db_len..db_len + HASH_LEN];

    // Step 6: the leftmost 8·emLen − emBits bits of maskedDB must be zero.
    let unused_bits = 8 * em_len - em_bits;
    let unused_mask = if unused_bits == 0 {
        0
    } else {
        0xff_u8 << (8 - unused_bits)
    };
    consistent &= masked_db[0] & unused_mask == 0;

    // Step 7: dbMask = MGF(H, emLen - hLen - 1).
    let mut db_mask = mgf1_sha256(h, db_len)?;

    // Step 8: DB = maskedDB xor dbMask.
    let mut db: Vec<u8> = masked_db
        .iter()
        .zip(db_mask.iter())
        .map(|(masked, mask)| masked ^ mask)
        .collect();
    db_mask.zeroize();

    // Step 9: clear the leftmost 8·emLen − emBits bits of DB.
    db[0] &= !unused_mask;

    // Step 10: the leftmost emLen − hLen − sLen − 2 octets of DB are zero and the next is 0x01.
    let padding_len = em_len - HASH_LEN - salt_len - 2;
    consistent &= db[..padding_len].iter().all(|byte| *byte == 0);
    consistent &= db[padding_len] == 0x01;

    // Step 11: salt is the last sLen octets of DB.
    let salt = &db[db_len - salt_len..];

    // Steps 12–13: M' = 0x00 * 8 || mHash || salt; H' = Hash(M').
    let mut hash = Sha256::new();
    hash.update([0_u8; 8])?;
    hash.update(message_hash)?;
    hash.update(salt)?;
    let h_prime = hash.finalize();

    // Step 14: consistent exactly when H == H'.
    consistent &= h_prime.as_bytes() == h;
    db.zeroize();

    if consistent {
        Ok(())
    } else {
        Err(CryptoError::InvalidSignature)
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    /// Build a valid EM by the §9.1.1 encoding steps so each verify check can be exercised.
    fn encode(message_hash: &[u8; 32], salt: &[u8], em_bits: usize) -> Vec<u8> {
        let em_len = em_bits.div_ceil(8);
        let mut hash = Sha256::new();
        hash.update([0_u8; 8]).unwrap();
        hash.update(message_hash).unwrap();
        hash.update(salt).unwrap();
        let h = hash.finalize().into_bytes();
        let db_len = em_len - HASH_LEN - 1;
        let mut db = alloc::vec![0_u8; db_len];
        db[db_len - salt.len() - 1] = 0x01;
        db[db_len - salt.len()..].copy_from_slice(salt);
        let mask = mgf1_sha256(&h, db_len).unwrap();
        let mut em: Vec<u8> = db.iter().zip(mask.iter()).map(|(a, b)| a ^ b).collect();
        let unused_bits = 8 * em_len - em_bits;
        if unused_bits != 0 {
            em[0] &= 0xff >> unused_bits;
        }
        em.extend_from_slice(&h);
        em.push(0xbc);
        em
    }

    #[test]
    fn locally_encoded_messages_verify_and_each_check_rejects_its_defect() {
        let m_hash = [0x5a; 32];
        let salt = [0x11; 32];
        let em_bits = 2047;
        let em = encode(&m_hash, &salt, em_bits);
        assert!(emsa_pss_verify_sha256(&m_hash, &em, em_bits, 32).is_ok());

        let mut wrong_trailer = em.clone();
        wrong_trailer[255] = 0xbd;
        assert!(emsa_pss_verify_sha256(&m_hash, &wrong_trailer, em_bits, 32).is_err());

        let mut high_bit = em.clone();
        high_bit[0] |= 0x80;
        assert!(emsa_pss_verify_sha256(&m_hash, &high_bit, em_bits, 32).is_err());

        assert!(emsa_pss_verify_sha256(&m_hash, &em, em_bits, 20).is_err());
        assert!(emsa_pss_verify_sha256(&[0x5b; 32], &em, em_bits, 32).is_err());
        assert!(emsa_pss_verify_sha256(&m_hash, &em[..255], em_bits, 32).is_err());
        assert!(emsa_pss_verify_sha256(&m_hash, &em, em_bits, 222).is_err());
    }

    #[test]
    fn zero_length_salt_and_byte_aligned_em_bits_are_supported() {
        let m_hash = [0x33; 32];
        let em = encode(&m_hash, &[], 2048);
        assert!(emsa_pss_verify_sha256(&m_hash, &em, 2048, 0).is_ok());
        assert!(emsa_pss_verify_sha256(&m_hash, &em, 2048, 1).is_err());
    }
}
