//! Public HKDF output-bound and composition behavior.

use rsl_crypto::{
    CryptoError,
    kdf::hkdf::sha256::{HkdfSha256Prk, derive, extract},
};

/// Boundary evidence that the maximum one-octet counter value succeeds.
#[test]
fn exactly_255_blocks_succeeds() {
    let prk = extract(Some(b"salt"), b"input keying material").expect("the fixture is short");
    let mut output = vec![0_u8; HkdfSha256Prk::MAX_OUTPUT_LEN];

    prk.expand(b"context", &mut output)
        .expect("RFC 5869 permits exactly 255 SHA-256 blocks");

    assert!(output.iter().any(|byte| *byte != 0));
}

/// Boundary evidence that output rejection occurs before any caller byte changes.
#[test]
fn more_than_255_blocks_is_rejected_atomically() {
    let prk = extract(Some(b"salt"), b"input keying material").expect("the fixture is short");
    let mut output = vec![0xa5; HkdfSha256Prk::MAX_OUTPUT_LEN + 1];

    assert_eq!(
        prk.expand(b"context", &mut output),
        Err(CryptoError::OutputTooLong)
    );
    assert!(output.iter().all(|byte| *byte == 0xa5));
}

/// Regression evidence that the convenience operation preserves the explicit stage composition.
#[test]
fn derive_matches_explicit_extract_then_expand() {
    let mut composed = [0_u8; 65];
    let mut convenient = [0_u8; 65];
    let prk = extract(None, b"input keying material").expect("the fixture is short");
    prk.expand(b"context", &mut composed)
        .expect("the requested output is short");

    derive(None, b"input keying material", b"context", &mut convenient)
        .expect("the requested output is short");

    assert_eq!(convenient, composed);
}
