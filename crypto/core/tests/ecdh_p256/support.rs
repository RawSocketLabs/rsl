//! Mechanical conversion helpers for published hexadecimal fixtures.

use rsl_crypto::agreement::ecdh_p256::EcdhP256PublicKey;

/// Decode exactly `2 * N` hexadecimal digits into their printed big-endian byte order.
pub(crate) fn decode<const N: usize>(encoded: &str) -> [u8; N] {
    assert_eq!(encoded.len(), N * 2, "fixture has {N} encoded bytes");
    core::array::from_fn(|byte_index| {
        let first = encoded.as_bytes()[byte_index * 2];
        let second = encoded.as_bytes()[byte_index * 2 + 1];
        (nibble(first) << 4) | nibble(second)
    })
}

/// Assemble SEC 1 uncompressed `04 || x || y` from two printed coordinates.
pub(crate) fn uncompressed(x: &str, y: &str) -> [u8; 65] {
    let mut bytes = [0_u8; 65];
    bytes[0] = 0x04;
    bytes[1..33].copy_from_slice(&decode::<32>(x));
    bytes[33..].copy_from_slice(&decode::<32>(y));
    bytes
}

/// Parse a public key from printed coordinates, panicking on a fixture that should be valid.
pub(crate) fn public_key(x: &str, y: &str) -> EcdhP256PublicKey {
    EcdhP256PublicKey::from_bytes(uncompressed(x, y)).expect("published point is on the curve")
}

fn nibble(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        b'A'..=b'F' => digit - b'A' + 10,
        _ => panic!("fixture contains a non-hexadecimal digit"),
    }
}

#[test]
fn conversion_retains_leading_zeroes_and_printed_byte_order() {
    let decoded = decode::<4>("000102ff");
    assert_eq!(decoded, [0x00, 0x01, 0x02, 0xff]);
    assert_eq!(decode::<2>("ABcd"), [0xab, 0xcd]);
}
