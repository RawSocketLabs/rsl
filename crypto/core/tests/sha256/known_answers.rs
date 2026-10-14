//! Published SHA-256 known-answer tests through the public API.
//!
//! Both messages and digests are published in NIST's *Secure Hash Algorithm — Message Digest
//! Length = 256* intermediate-value example. Exact source metadata is recorded in
//! `tests/vectors/sha256/README.md`.

use super::support::assert_sha256;

/// Published known-answer evidence: NIST's one-block `abc` sample.
#[test]
fn nist_one_block_message() {
    assert_sha256(
        "abc",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
}

/// Published known-answer evidence: NIST's two-block message sample.
#[test]
fn nist_two_block_message() {
    assert_sha256(
        "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    );
}
