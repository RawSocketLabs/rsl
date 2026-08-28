//! Public full-tag verification behavior.

use rsl_crypto::{CryptoError, mac::hmac::sha256::HmacSha256};

/// Public-contract evidence that the exact full tag verifies.
#[test]
fn exact_tag_verifies() {
    let expected = HmacSha256::authenticate(b"key", "message")
        .expect("the fixture is short")
        .into_bytes();
    let mut state = HmacSha256::new(b"key").expect("the fixture key is short");
    state.update("message").expect("the fixture is short");

    assert_eq!(state.verify(expected), Ok(()));
}

/// Negative evidence that wrong values and wrong lengths share one error category.
#[test]
fn every_mismatch_returns_authentication_failed() {
    let correct = HmacSha256::authenticate(b"key", "message")
        .expect("the fixture is short")
        .into_bytes();

    for index in [0, correct.len() / 2, correct.len() - 1] {
        let mut wrong = correct;
        wrong[index] ^= 1;
        assert_eq!(
            state_for_fixture().verify(wrong),
            Err(CryptoError::AuthenticationFailed)
        );
    }

    assert_eq!(
        state_for_fixture().verify(&correct[..correct.len() - 1]),
        Err(CryptoError::AuthenticationFailed)
    );

    let mut too_long = [0_u8; 33];
    too_long[..correct.len()].copy_from_slice(&correct);
    assert_eq!(
        state_for_fixture().verify(too_long),
        Err(CryptoError::AuthenticationFailed)
    );
}

/// Rebuild the same state because verification deliberately consumes it.
fn state_for_fixture() -> HmacSha256 {
    let mut state = HmacSha256::new(b"key").expect("the fixture key is short");
    state.update("message").expect("the fixture is short");
    state
}
