//! Differential comparison with the independent `RustCrypto` SHA-256 implementation.
//!
//! `sha2` is a development-only dependency and is never called by `rsl-crypto` production code.
//! This evidence can catch mistakes not exercised by the finite published-vector set; it is not
//! a substitute for those vectors or for an independent cryptographic review.

use rsl_crypto::digest::sha2::sha256::Sha256;
use sha2::{Digest as ReferenceDigest, Sha256 as ReferenceSha256};

/// Differential evidence across padding, block, multi-block, and longer-message lengths.
#[test]
fn deterministic_messages_match_rustcrypto_sha256() {
    for length in [
        0, 1, 2, 3, 7, 31, 54, 55, 56, 57, 62, 63, 64, 65, 66, 119, 120, 121, 127, 128, 129, 255,
        256, 257, 511, 512, 513, 1_024, 4_096,
    ] {
        let message = deterministic_message(length);
        let expected = ReferenceSha256::digest(&message);
        let actual = Sha256::digest(&message).expect("the test message is within SHA-256 limits");

        assert_eq!(
            actual.as_ref(),
            &expected[..],
            "implementations differ for a {length}-byte message"
        );
    }
}

/// Differential evidence that fragmented input agrees with an independent one-shot oracle.
#[test]
fn fragmented_messages_match_rustcrypto_sha256() {
    let message = deterministic_message(1_031);
    let expected = ReferenceSha256::digest(&message);

    for fragment_len in [1, 7, 55, 56, 63, 64, 65, 128, 257] {
        let mut actual = Sha256::new();

        for fragment in message.chunks(fragment_len) {
            actual
                .update(fragment)
                .expect("the test message is within SHA-256 limits");
        }

        assert_eq!(
            actual.finalize().as_ref(),
            &expected[..],
            "implementations differ for {fragment_len}-byte fragments"
        );
    }
}

/// Produce deterministic, nonrepeating-enough bytes for differential coverage.
fn deterministic_message(length: usize) -> Vec<u8> {
    let mut state = 0x6d_2b_79_f5_u32;

    (0..length)
        .map(|_| {
            // A fixed xorshift sequence makes failures reproducible. It is test data generation,
            // not cryptographic randomness.
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        })
        .collect()
}
