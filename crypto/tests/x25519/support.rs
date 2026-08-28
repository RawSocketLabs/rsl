//! Mechanical conversion helpers for the RFC's hexadecimal byte strings.

/// Decode exactly 64 hexadecimal digits into their printed 32-byte order.
pub(crate) fn decode_32(encoded: &str) -> [u8; 32] {
    assert_eq!(encoded.len(), 64, "an X25519 fixture has 32 encoded bytes");

    core::array::from_fn(|byte_index| {
        let first_digit = encoded.as_bytes()[byte_index * 2];
        let second_digit = encoded.as_bytes()[byte_index * 2 + 1];
        (nibble(first_digit) << 4) | nibble(second_digit)
    })
}

/// Convert one ASCII hexadecimal digit without changing byte order.
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
    let decoded = decode_32("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");

    assert_eq!(
        decoded,
        core::array::from_fn(|index| {
            u8::try_from(index).expect("every X25519 byte index fits in u8")
        })
    );
}
