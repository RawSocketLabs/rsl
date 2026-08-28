//! Public tests for HMAC input representations and incremental fragmentation.

use rsl_crypto::mac::hmac::sha384::{HmacSha384, HmacSha384Tag};

/// Regression evidence: text and byte containers expose only their existing byte representation.
#[test]
fn common_key_and_message_representations_agree() {
    let expected = HmacSha384::authenticate(b"key", "message")
        .expect("the fixture is short")
        .into_bytes();
    let key = String::from("key");
    let message = String::from("message");
    let key_bytes = Vec::from(*b"key");
    let message_bytes = Vec::from(*b"message");

    assert_eq!(
        HmacSha384::authenticate(key.as_bytes(), &message).map(HmacSha384Tag::into_bytes),
        Ok(expected)
    );
    assert_eq!(
        HmacSha384::authenticate(key.as_bytes(), message).map(HmacSha384Tag::into_bytes),
        Ok(expected)
    );
    assert_eq!(
        HmacSha384::authenticate(&key_bytes, message_bytes).map(HmacSha384Tag::into_bytes),
        Ok(expected)
    );
}

/// Regression evidence: fragmentation does not alter the authenticated byte string.
#[test]
fn awkward_fragment_sizes_match_one_shot_authentication() {
    let key = deterministic_bytes(131);
    let message = deterministic_bytes(521);
    let expected =
        HmacSha384::authenticate(&key, &message).expect("the deterministic fixture is short");

    for fragment_len in [1, 2, 3, 7, 31, 55, 56, 63, 64, 65, 127, 256] {
        assert_eq!(fragmented_tag(&key, &message, fragment_len), expected);
    }
}

/// Regression evidence: an empty update adds no message bytes.
#[test]
fn empty_update_is_a_no_op() {
    let mut state = HmacSha384::new(b"key").expect("the fixture key is short");
    state.update("mes").expect("the fixture is short");
    state.update([]).expect("an empty update always fits");
    state.update("sage").expect("the fixture is short");

    assert_eq!(
        state.finalize(),
        HmacSha384::authenticate(b"key", "message").expect("the fixture is short")
    );
}

/// Authenticate one message in fixed-size fragments.
fn fragmented_tag(key: &[u8], message: &[u8], fragment_len: usize) -> HmacSha384Tag {
    let mut state = HmacSha384::new(key).expect("the deterministic key is short");

    for fragment in message.chunks(fragment_len) {
        state
            .update(fragment)
            .expect("the deterministic message is short");
    }

    state.finalize()
}

/// Produce reproducible bytes for representation and fragmentation coverage.
fn deterministic_bytes(length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| {
            let index = u8::try_from(index % 256).expect("the remainder is always at most 255");
            index.wrapping_mul(29).wrapping_add(113)
        })
        .collect()
}
