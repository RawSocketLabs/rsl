//! Public tests for input representations and incremental fragmentation.

use rsl_crypto::digest::sha2::sha256::{Sha256, Sha256Digest};

/// Regression evidence: common owned and borrowed byte-like inputs represent the same message.
#[test]
fn one_shot_api_accepts_common_byte_representations() {
    let expected = Sha256::digest("abc")
        .expect("three bytes are within SHA-256 limits")
        .into_bytes();
    let owned_text = String::from("abc");
    let owned_bytes = Vec::from(*b"abc");
    let byte_array = *b"abc";
    let byte_slice = byte_array.as_slice();

    assert_eq!(
        Sha256::digest(&owned_text).map(Sha256Digest::into_bytes),
        Ok(expected)
    );
    assert_eq!(
        Sha256::digest(owned_text).map(Sha256Digest::into_bytes),
        Ok(expected)
    );
    assert_eq!(
        Sha256::digest(&owned_bytes).map(Sha256Digest::into_bytes),
        Ok(expected)
    );
    assert_eq!(
        Sha256::digest(owned_bytes).map(Sha256Digest::into_bytes),
        Ok(expected)
    );
    assert_eq!(
        Sha256::digest(byte_array).map(Sha256Digest::into_bytes),
        Ok(expected)
    );
    assert_eq!(
        Sha256::digest(byte_slice).map(Sha256Digest::into_bytes),
        Ok(expected)
    );
}

/// Regression evidence: arbitrary update fragmentation does not change the represented message.
#[test]
fn awkward_fragment_sizes_match_one_shot_input() {
    let message = deterministic_message(257);
    let expected = Sha256::digest(&message).expect("the test message is within SHA-256 limits");

    for fragment_len in [1, 2, 3, 7, 31, 55, 56, 63, 64, 65, 127] {
        assert_eq!(fragmented_digest(&message, fragment_len), expected);
    }
}

/// Regression evidence: an empty update is a no-op even between nonempty fragments.
#[test]
fn empty_updates_do_not_change_the_message() {
    let mut state = Sha256::new();
    state.update("ab").expect("two bytes fit");
    state.update([]).expect("an empty update always fits");
    state.update("c").expect("one additional byte fits");

    assert_eq!(
        state.finalize(),
        Sha256::digest("abc").expect("three bytes fit")
    );
}

/// Hash a message in fixed-size fragments through the public incremental API.
fn fragmented_digest(message: &[u8], fragment_len: usize) -> Sha256Digest {
    let mut state = Sha256::new();

    for fragment in message.chunks(fragment_len) {
        state
            .update(fragment)
            .expect("the test message is within SHA-256 limits");
    }

    state.finalize()
}

/// Build reproducible nonuniform bytes without introducing a random-number dependency.
fn deterministic_message(length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| {
            let index = u8::try_from(index % 256).expect("the remainder is always at most 255");
            index.wrapping_mul(73).wrapping_add(41)
        })
        .collect()
}
