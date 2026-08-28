//! RFC 4231 HMAC-SHA-256 vectors through the public API.
//!
//! The exact publication metadata and fixture-conversion policy are recorded in
//! `tests/vectors/hmac-sha256/README.md`.

use rsl_crypto::mac::hmac::sha256::HmacSha256;

use super::support::{assert_hmac_sha256, decode_hex};

/// Published vector evidence: RFC 4231 §4.2, Test Case 1.
#[test]
fn test_case_1() {
    assert_hmac_sha256(
        &[0x0b; 20],
        decode_hex::<8>("4869205468657265"),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
    );
}

/// Published vector evidence: RFC 4231 §4.3, Test Case 2.
#[test]
fn test_case_2() {
    assert_hmac_sha256(
        &decode_hex::<4>("4a656665"),
        decode_hex::<28>("7768617420646f2079612077616e7420666f72206e6f7468696e673f"),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
    );
}

/// Published vector evidence: RFC 4231 §4.4, Test Case 3.
#[test]
fn test_case_3() {
    assert_hmac_sha256(
        &[0xaa; 20],
        [0xdd; 50],
        "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe",
    );
}

/// Published vector evidence: RFC 4231 §4.5, Test Case 4.
#[test]
fn test_case_4() {
    let key: [u8; 25] = core::array::from_fn(|index| {
        u8::try_from(index + 1).expect("RFC 4231 Test Case 4 key bytes are 1 through 25")
    });

    assert_hmac_sha256(
        &key,
        [0xcd; 50],
        "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b",
    );
}

/// Published prefix evidence: RFC 4231 §4.6, Test Case 5.
#[test]
fn test_case_5_publishes_only_a_truncated_prefix() {
    let tag = HmacSha256::authenticate(
        &[0x0c; 20],
        decode_hex::<20>("546573742057697468205472756e636174696f6e"),
    )
    .expect("the published fixture is within SHA-256 limits");
    let published_prefix = decode_hex::<16>("a3b6167473100ee06e0c796c2955552b");

    assert_eq!(&tag.as_bytes()[..published_prefix.len()], &published_prefix);
}

/// Published vector evidence: RFC 4231 §4.7, Test Case 6.
#[test]
fn test_case_6_hashes_a_long_key_first() {
    assert_hmac_sha256(
        &[0xaa; 131],
        decode_hex::<54>(
            "54657374205573696e67204c6172676572205468616e20426c6f636b2d53697a\
             65204b6579202d2048617368204b6579204669727374",
        ),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54",
    );
}

/// Published vector evidence: RFC 4231 §4.8, Test Case 7.
#[test]
fn test_case_7_hashes_a_long_key_and_long_message() {
    assert_hmac_sha256(
        &[0xaa; 131],
        decode_hex::<152>(
            "5468697320697320612074657374207573696e672061206c6172676572207468\
             616e20626c6f636b2d73697a65206b657920616e642061206c61726765722074\
             68616e20626c6f636b2d73697a6520646174612e20546865206b6579206e6565\
             647320746f20626520686173686564206265666f7265206265696e6720757365\
             642062792074686520484d414320616c676f726974686d2e",
        ),
        "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2",
    );
}
