//! Small fixture helpers shared by the public SHA-256 tests.

use rsl_crypto::digest::sha2::sha256::Sha256;

/// Decode an exact-size hexadecimal fixture without adding a test dependency.
pub(crate) fn decode_hex<const N: usize>(encoded: &str) -> [u8; N] {
    assert_eq!(
        encoded.len(),
        N * 2,
        "a {N}-byte fixture must contain exactly {} hexadecimal digits",
        N * 2
    );

    let encoded = encoded.as_bytes();
    core::array::from_fn(|index| {
        let high = decode_nibble(encoded[index * 2]);
        let low = decode_nibble(encoded[index * 2 + 1]);
        (high << 4) | low
    })
}

/// Compare the public one-shot API with an expected hexadecimal digest.
pub(crate) fn assert_sha256(message: impl AsRef<[u8]>, expected_hex: &str) {
    let expected = decode_hex::<32>(expected_hex);
    let actual = Sha256::digest(message).expect("the test message is within SHA-256 limits");

    assert_eq!(actual.into_bytes(), expected);
}

/// Convert one ASCII hexadecimal digit into its numeric value.
fn decode_nibble(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        b'A'..=b'F' => digit - b'A' + 10,
        _ => panic!("test fixture contains a non-hexadecimal digit"),
    }
}
