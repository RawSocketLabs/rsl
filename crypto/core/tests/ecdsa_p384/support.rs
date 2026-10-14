//! Mechanical conversion helpers for published hexadecimal fixtures.

use rsl_crypto::signature::ecdsa_p384::{EcdsaP384Signature, EcdsaP384VerifyingKey};

/// Decode exactly `2 * N` hexadecimal digits into their printed big-endian byte order.
pub(crate) fn decode<const N: usize>(encoded: &str) -> [u8; N] {
    assert_eq!(encoded.len(), N * 2, "fixture has {N} encoded bytes");
    core::array::from_fn(|byte_index| {
        let first = encoded.as_bytes()[byte_index * 2];
        let second = encoded.as_bytes()[byte_index * 2 + 1];
        (nibble(first) << 4) | nibble(second)
    })
}

/// Decode a variable-length printed message into its byte order.
pub(crate) fn decode_vec(encoded: &str) -> Vec<u8> {
    assert_eq!(encoded.len() % 2, 0, "fixture has whole bytes");
    encoded
        .as_bytes()
        .chunks(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect()
}

/// Assemble SEC 1 uncompressed `04 || x || y` from two printed coordinates.
pub(crate) fn uncompressed(x: &str, y: &str) -> [u8; 97] {
    let mut bytes = [0_u8; 97];
    bytes[0] = 0x04;
    bytes[1..49].copy_from_slice(&decode::<48>(x));
    bytes[49..].copy_from_slice(&decode::<48>(y));
    bytes
}

/// Parse a verifying key from printed coordinates, panicking on a fixture that should be valid.
pub(crate) fn verifying_key(x: &str, y: &str) -> EcdsaP384VerifyingKey {
    EcdsaP384VerifyingKey::from_bytes(uncompressed(x, y)).expect("published point is on the curve")
}

/// Assemble `r || s` from two printed scalars.
pub(crate) fn signature(r: &str, s: &str) -> EcdsaP384Signature {
    let mut bytes = [0_u8; 96];
    bytes[..48].copy_from_slice(&decode::<48>(r));
    bytes[48..].copy_from_slice(&decode::<48>(s));
    EcdsaP384Signature::from_bytes(bytes)
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
    assert_eq!(decode::<4>("000102ff"), [0x00, 0x01, 0x02, 0xff]);
    assert_eq!(decode_vec("ABcd"), vec![0xab, 0xcd]);
}
