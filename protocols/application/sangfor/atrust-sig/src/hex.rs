//! Hexadecimal text, in the two letter cases the protocol uses.

/// `bytes` as hexadecimal digits, uppercase or lowercase.
pub(crate) fn encode(bytes: &[u8], uppercase: bool) -> String {
    let digits: &[u8; 16] = if uppercase {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    bytes
        .iter()
        .flat_map(|byte| {
            [
                digits[usize::from(byte >> 4)],
                digits[usize::from(byte & 15)],
            ]
        })
        .map(char::from)
        .collect()
}

/// The bytes an unsigned hexadecimal number denotes, most significant first. An odd number
/// of digits is read as if a zero preceded it, as a big-integer parser would.
pub(crate) fn decode_number(text: &str) -> Option<Vec<u8>> {
    let digit = |byte: u8| {
        (byte as char)
            .to_digit(16)
            .and_then(|value| u8::try_from(value).ok())
    };
    let digits: Option<Vec<u8>> = text.bytes().map(digit).collect();
    let digits = digits.filter(|digits| !digits.is_empty())?;
    let odd = digits.len() % 2;
    let first = (odd == 1).then(|| digits[0]);
    let pairs = digits[odd..].chunks(2).map(|pair| pair[0] << 4 | pair[1]);
    Some(first.into_iter().chain(pairs).collect())
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn encodes_in_the_case_asked_for() {
        assert_eq!(encode(&[0x00, 0xab, 0xff], true), "00ABFF");
        assert_eq!(encode(&[0x00, 0xab, 0xff], false), "00abff");
        assert_eq!(encode(&[], true), "");
    }

    #[test]
    fn decodes_numbers_of_any_digit_count_and_case() {
        assert_eq!(
            decode_number("00C0ffee"),
            Some(vec![0x00, 0xc0, 0xff, 0xee])
        );
        assert_eq!(decode_number("abc"), Some(vec![0x0a, 0xbc]));
        assert_eq!(decode_number("7"), Some(vec![0x07]));
        assert_eq!(decode_number(""), None);
        assert_eq!(decode_number("0x10"), None);
        assert_eq!(decode_number("12 "), None);
    }
}
