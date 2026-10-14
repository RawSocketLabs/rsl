//! RFC 4231 HMAC-SHA-384 vectors through the public API.
//!
//! The exact publication metadata and fixture-conversion policy are recorded in
//! `tests/vectors/hmac-sha384/README.md`.

use rsl_crypto::mac::hmac::sha384::HmacSha384;

use super::support::{assert_hmac_sha384, decode_hex};

fn decode(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("fixture is hex"))
        .collect()
}

/// Published vector evidence: RFC 4231 §4.2, Test Case 1.
#[test]
fn test_case_1() {
    assert_hmac_sha384(
        &decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"),
        decode("4869205468657265"),
        "afd03944d84895626b0825f4ab46907f15f9dadbe4101ec682aa034c7cebc59cfaea9ea9076ede7f4af152e8b2fa9cb6",
    );
}

/// Published vector evidence: RFC 4231 §4.3, Test Case 2.
#[test]
fn test_case_2() {
    assert_hmac_sha384(
        &decode("4a656665"),
        decode("7768617420646f2079612077616e7420666f72206e6f7468696e673f"),
        "af45d2e376484031617f78d2b58a6b1b9c7ef464f5a01b47e42ec3736322445e8e2240ca5e69e2c78b3239ecfab21649",
    );
}

/// Published vector evidence: RFC 4231 §4.4, Test Case 3.
#[test]
fn test_case_3() {
    assert_hmac_sha384(
        &decode("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        decode(
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        ),
        "88062608d3e6ad8a0aa2ace014c8a86f0aa635d947ac9febe83ef4e55966144b2a5ab39dc13814b94e3ab6e101a34f27",
    );
}

/// Published vector evidence: RFC 4231 §4.5, Test Case 4.
#[test]
fn test_case_4() {
    assert_hmac_sha384(
        &decode("0102030405060708090a0b0c0d0e0f10111213141516171819"),
        decode(
            "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
        ),
        "3e8a69b7783c25851933ab6290af6ca77a9981480850009cc5577c6e1f573b4e6801dd23c4a7d679ccf8a386c674cffb",
    );
}

/// Published vector evidence: RFC 4231 §4.6, Test Case 5, which publishes only the leftmost 128
/// bits; the full tag's prefix must match.
#[test]
fn test_case_5_truncated_prefix() {
    let tag = HmacSha384::authenticate(
        &decode("0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"),
        decode("546573742057697468205472756e636174696f6e"),
    )
    .expect("the fixture is within SHA-384 limits");
    assert_eq!(
        &tag.as_bytes()[..16],
        &decode_hex::<16>("3abf34c3503b2a23a46efc619baef897")
    );
}

/// Published vector evidence: RFC 4231 §4.7, Test Case 6.
#[test]
fn test_case_6() {
    assert_hmac_sha384(
        &decode(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        decode(
            "54657374205573696e67204c6172676572205468616e20426c6f636b2d53697a65204b6579202d2048617368204b6579204669727374",
        ),
        "4ece084485813e9088d2c63a041bc5b44f9ef1012a2b588f3cd11f05033ac4c60c2ef6ab4030fe8296248df163f44952",
    );
}

/// Published vector evidence: RFC 4231 §4.8, Test Case 7.
#[test]
fn test_case_7() {
    assert_hmac_sha384(
        &decode(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        decode(
            "5468697320697320612074657374207573696e672061206c6172676572207468616e20626c6f636b2d73697a65206b657920616e642061206c6172676572207468616e20626c6f636b2d73697a6520646174612e20546865206b6579206e6565647320746f20626520686173686564206265666f7265206265696e6720757365642062792074686520484d414320616c676f726974686d2e",
        ),
        "6617178e941f020d351e2f254e8fd32c602420feb0b8fb9adccebb82461e99c5a678cc31e799176d3860e6110c46523e",
    );
}
