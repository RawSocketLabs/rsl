//! Published CAVP vectors around SHA-256's padding and block boundaries.
//!
//! These cases are transcribed from `SHA256ShortMsg.rsp` in NIST's byte-oriented Secure Hash
//! Algorithm test-vector archive. The selected lengths exercise the empty-message case, a
//! nonzero byte, both sides of the 55/56-byte padding transition, and both sides of the
//! 63/64-byte compression-block transition. Exact archive provenance is recorded in
//! `tests/vectors/sha256/README.md`.

use super::support::{assert_sha256, decode_hex};

/// Apply a published byte-oriented vector with a compile-time-checked message size.
fn assert_cavp_case<const N: usize>(message_hex: &str, digest_hex: &str) {
    assert_sha256(decode_hex::<N>(message_hex), digest_hex);
}

/// Published vector evidence: `SHA256ShortMsg.rsp`, `Len = 0`.
#[test]
fn empty_message() {
    // CAVP writes `Msg = 00` as a placeholder for the zero-bit message. It is deliberately not
    // decoded because the declared message length is zero.
    assert_sha256(
        [],
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
}

/// Published vector evidence: `SHA256ShortMsg.rsp`, `Len = 8`.
#[test]
fn one_byte_message() {
    assert_cavp_case::<1>(
        "d3",
        "28969cdfa74a12c82f3bad960b0b000aca2ac329deea5c2328ebc6f2ba9802c1",
    );
}

/// Published vector evidence: `SHA256ShortMsg.rsp`, `Len = 440` (55 bytes).
#[test]
fn fifty_five_byte_message() {
    assert_cavp_case::<55>(
        "3ebfb06db8c38d5ba037f1363e118550aad94606e26835a01af05078533cc25f\
         2f39573c04b632f62f68c294ab31f2a3e2a1a0d8c2be51",
        "6595a2ef537a69ba8583dfbf7f5bec0ab1f93ce4c8ee1916eff44a93af5749c4",
    );
}

/// Published vector evidence: `SHA256ShortMsg.rsp`, `Len = 448` (56 bytes).
#[test]
fn fifty_six_byte_message() {
    assert_cavp_case::<56>(
        "2d52447d1244d2ebc28650e7b05654bad35b3a68eedc7f8515306b496d75f3e7\
         3385dd1b002625024b81a02f2fd6dffb6e6d561cb7d0bd7a",
        "cfb88d6faf2de3a69d36195acec2e255e2af2b7d933997f348e09f6ce5758360",
    );
}

/// Published vector evidence: `SHA256ShortMsg.rsp`, `Len = 504` (63 bytes).
#[test]
fn sixty_three_byte_message() {
    assert_cavp_case::<63>(
        "e2f76e97606a872e317439f1a03fcd92e632e5bd4e7cbc4e97f1afc19a16fde9\
         2d77cbe546416b51640cddb92af996534dfd81edb17c4424cf1ac4d75aceeb",
        "18041bd4665083001fba8c5411d2d748e8abbfdcdfd9218cb02b68a78e7d4c23",
    );
}

/// Published vector evidence: `SHA256ShortMsg.rsp`, `Len = 512` (64 bytes).
#[test]
fn sixty_four_byte_message() {
    assert_cavp_case::<64>(
        "5a86b737eaea8ee976a0a24da63e7ed7eefad18a101c1211e2b3650c5187c2a8\
         a650547208251f6d4237e661c7bf4c77f335390394c37fa1a9f9be836ac28509",
        "42e61e174fbb3897d6dd6cef3dd2802fe67b331953b06114a65c772859dfc1aa",
    );
}
