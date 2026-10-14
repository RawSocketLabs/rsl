//! Published `ChaCha20` evidence from RFC 8439 Appendix A.1 and A.2.

use rsl_crypto::{
    CryptoError,
    cipher::chacha20::{ChaCha20, ChaCha20Key, ChaCha20Nonce},
};

use crate::{
    rfc_fixtures::{BLOCK_CASES, ENCRYPTION_CASES},
    support,
};

/// Published evidence: all five A.1 block-function vectors, including counter 0 and the all-ones
/// key of vector 5.
#[test]
fn appendix_a1_block_functions() {
    for (index, case) in BLOCK_CASES.iter().enumerate() {
        let cipher = ChaCha20::new(ChaCha20Key::new(support::decode_array(case.key)));
        let nonce = ChaCha20Nonce::new(support::decode_array(case.nonce));
        assert_eq!(
            cipher.keystream_block(case.counter, &nonce).as_slice(),
            support::decode(case.keystream).as_slice(),
            "A.1 vector {}",
            index + 1
        );
    }
}

/// Published evidence: all three A.2 encryptions, including the 42-block start of vector 3 and
/// the multi-block vector 2.
#[test]
fn appendix_a2_encryptions_round_trip() {
    for (index, case) in ENCRYPTION_CASES.iter().enumerate() {
        let cipher = ChaCha20::new(ChaCha20Key::new(support::decode_array(case.key)));
        let nonce = ChaCha20Nonce::new(support::decode_array(case.nonce));
        let plaintext = support::decode(case.plaintext);
        let ciphertext = support::decode(case.ciphertext);
        assert_eq!(
            cipher.encrypt(case.counter, &nonce, &plaintext).unwrap(),
            ciphertext,
            "A.2 vector {} encrypt",
            index + 1
        );
        assert_eq!(
            cipher.encrypt(case.counter, &nonce, &ciphertext).unwrap(),
            plaintext,
            "A.2 vector {} decrypt",
            index + 1
        );
    }
}

/// Standard-derived evidence: counter exhaustion and exact nonce length at the public boundary.
#[test]
fn counter_and_nonce_boundaries() {
    let cipher = ChaCha20::new(ChaCha20Key::new([0x42; 32]));
    let nonce = ChaCha20Nonce::new([0; 12]);
    assert_eq!(
        cipher.encrypt(u32::MAX, &nonce, &[0; 65]),
        Err(CryptoError::CounterExhausted)
    );
    assert!(cipher.encrypt(u32::MAX, &nonce, &[0; 64]).is_ok());
    assert_eq!(
        ChaCha20Nonce::try_from([0_u8; 8].as_slice()),
        Err(CryptoError::InvalidLength {
            name: "ChaCha20 nonce",
            expected: 12,
            actual: 8,
        })
    );
}
