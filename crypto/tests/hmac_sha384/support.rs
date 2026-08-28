//! Small fixture helpers shared by public HMAC-SHA-384 tests.

use rsl_crypto::mac::hmac::sha384::HmacSha384;

/// Decode an exact-size hexadecimal fixture without adding a general-purpose dependency.
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

/// Compare the public one-shot API with a full published hexadecimal tag.
pub(crate) fn assert_hmac_sha384(key: &[u8], message: impl AsRef<[u8]>, expected_hex: &str) {
    let expected = decode_hex::<48>(expected_hex);
    let actual = HmacSha384::authenticate(key, message)
        .expect("the published fixture is within SHA-384 limits");

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
