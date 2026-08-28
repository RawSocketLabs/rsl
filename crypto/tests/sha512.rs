//! Public evidence for the readable SHA-512 implementation.

use rsl_crypto::digest::sha2::sha512::Sha512;
use sha2::{Digest as _, Sha512 as ReferenceSha512};

/// Published FIPS 180-4 one-block and two-block examples.
#[test]
fn published_examples() {
    let cases = [
        (
            "abc",
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
        ),
        (
            "abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmn\
             hijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018\
             501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909",
        ),
    ];

    for (message, expected) in cases {
        assert_eq!(
            Sha512::digest(message).unwrap().into_bytes(),
            hex64(expected)
        );
    }
}

/// Differential evidence over every important padding and block boundary.
#[test]
fn varied_and_fragmented_messages_match_rustcrypto() {
    for length in [
        0_usize, 1, 2, 7, 31, 110, 111, 112, 113, 127, 128, 129, 255, 256, 1024, 4096,
    ] {
        let message: Vec<u8> = (0..length)
            .map(|index| index.to_le_bytes()[0].wrapping_mul(73).wrapping_add(19))
            .collect();
        let expected = ReferenceSha512::digest(&message);
        let actual = Sha512::digest(&message).unwrap();
        assert_eq!(actual.as_ref(), expected.as_slice(), "length {length}");

        let mut fragmented = Sha512::new();
        for part in message.chunks(17) {
            fragmented.update(part).unwrap();
        }
        assert_eq!(
            fragmented.finalize().as_ref(),
            expected.as_slice(),
            "fragmented {length}"
        );
    }
}

/// Decode the exact published byte sequence retained in this test.
fn hex64(input: &str) -> [u8; 64] {
    let compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    core::array::from_fn(|index| {
        u8::from_str_radix(&compact[index * 2..index * 2 + 2], 16).expect("fixture is hex")
    })
}
