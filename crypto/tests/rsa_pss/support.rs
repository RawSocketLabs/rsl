//! Mechanical conversion helpers for published hexadecimal fixtures.

use rsl_crypto::signature::rsa_pss::RsaPssSignature;

/// Decode a printed hexadecimal string into its byte order, keeping leading zero bytes.
pub(crate) fn decode(encoded: &str) -> Vec<u8> {
    assert_eq!(encoded.len() % 2, 0, "fixture has whole bytes");
    encoded
        .as_bytes()
        .chunks(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect()
}

/// Wrap a printed signature.
pub(crate) fn signature(encoded: &str) -> RsaPssSignature {
    RsaPssSignature::from_bytes(decode(encoded))
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
    assert_eq!(decode("000102ff"), vec![0x00, 0x01, 0x02, 0xff]);
    assert!(decode("").is_empty());
}
