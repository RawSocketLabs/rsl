//! Differential HMAC-SHA-384 comparison with the independent `RustCrypto` implementation.
//!
//! The `hmac` and `sha2` crates are development-only dependencies. Production `rsl-crypto` code
//! calls neither implementation.

use hmac::{Hmac, KeyInit as _, Mac as ReferenceMac};
use rsl_crypto::mac::hmac::sha384::HmacSha384;
use sha2::Sha384 as ReferenceSha384;

/// Differential evidence across short, exact-block, and hashed-key paths and message boundaries.
#[test]
fn deterministic_keys_and_messages_match_rustcrypto() {
    for key_len in [0, 1, 20, 31, 32, 63, 64, 65, 131, 255] {
        let key = deterministic_bytes(key_len, 0x31);

        for message_len in [0, 1, 7, 55, 56, 63, 64, 65, 127, 128, 129, 521] {
            let message = deterministic_bytes(message_len, 0xa7);
            let mut reference = Hmac::<ReferenceSha384>::new_from_slice(&key)
                .expect("HMAC accepts keys of any byte length");
            reference.update(&message);
            let expected = reference.finalize().into_bytes();
            let actual = HmacSha384::authenticate(&key, &message)
                .expect("the deterministic fixture is within SHA-384 limits");

            assert_eq!(
                actual.as_ref(),
                &expected[..],
                "implementations differ for a {key_len}-byte key and {message_len}-byte message"
            );
        }
    }
}

/// Build deterministic nonuniform bytes without adding randomness to the test.
fn deterministic_bytes(length: usize, domain: u8) -> Vec<u8> {
    let mut state = u32::from(domain) | 0x6d2b_7900;

    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        })
        .collect()
}
