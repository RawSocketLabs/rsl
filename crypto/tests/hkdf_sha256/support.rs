//! Exact-size hexadecimal decoding for RFC 5869 fixtures.

pub(crate) fn decode_hex<const N: usize>(encoded: &str) -> [u8; N] {
    assert_eq!(encoded.len(), N * 2);
    let encoded = encoded.as_bytes();

    core::array::from_fn(|index| {
        (decode_nibble(encoded[index * 2]) << 4) | decode_nibble(encoded[index * 2 + 1])
    })
}

fn decode_nibble(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        b'A'..=b'F' => digit - b'A' + 10,
        _ => panic!("test fixture contains a non-hexadecimal digit"),
    }
}
