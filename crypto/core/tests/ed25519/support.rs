//! Exact hexadecimal conversion shared by the RFC fixtures.

pub(super) fn hex<const N: usize>(input: &str) -> [u8; N] {
    let compact: String = input
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    assert_eq!(
        compact.len(),
        N * 2,
        "fixture must encode exactly {N} bytes"
    );
    core::array::from_fn(|index| {
        u8::from_str_radix(&compact[index * 2..index * 2 + 2], 16).expect("fixture contains hex")
    })
}

#[test]
fn conversion_preserves_leading_zeroes_and_byte_order() {
    assert_eq!(hex::<4>("0001 feff"), [0x00, 0x01, 0xfe, 0xff]);
}
