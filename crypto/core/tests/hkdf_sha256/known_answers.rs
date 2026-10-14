//! RFC 5869 Appendix A SHA-256 vectors through the public Extract and Expand API.

use rsl_crypto::kdf::hkdf::sha256::extract;

use super::support::decode_hex;

/// Published vector evidence: RFC 5869 Appendix A.1.
#[test]
fn test_case_1() {
    let ikm = [0x0b; 22];
    let salt: [u8; 13] = core::array::from_fn(fixture_index);
    let info: [u8; 10] = core::array::from_fn(|index| 0xf0 + fixture_index(index));
    let expected_prk =
        decode_hex::<32>("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5");
    let expected_okm = decode_hex::<42>(
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
    );

    assert_case(Some(&salt), &ikm, &info, &expected_prk, &expected_okm);
}

/// Published vector evidence: RFC 5869 Appendix A.2.
#[test]
fn test_case_2() {
    let ikm: [u8; 80] = core::array::from_fn(fixture_index);
    let salt: [u8; 80] = core::array::from_fn(|index| 0x60 + fixture_index(index));
    let info: [u8; 80] = core::array::from_fn(|index| 0xb0 + fixture_index(index));
    let expected_prk =
        decode_hex::<32>("06a6b88c5853361a06104c9ceb35b45cef760014904671014a193f40c15fc244");
    let expected_okm = decode_hex::<82>(
        "b11e398dc80327a1c8e7f78c596a49344f012eda2d4efad8a050cc4c19afa97c\
         59045a99cac7827271cb41c65e590e09da3275600c2f09b8367793a9aca3db71\
         cc30c58179ec3e87c14c01d5c1f3434f1d87",
    );

    assert_case(Some(&salt), &ikm, &info, &expected_prk, &expected_okm);
}

/// Published vector evidence: RFC 5869 Appendix A.3.
#[test]
fn test_case_3() {
    let ikm = [0x0b; 22];
    let expected_prk =
        decode_hex::<32>("19ef24a32c717b167f33a91d6f648bdf96596776afdb6377ac434c1c293ccb04");
    let expected_okm = decode_hex::<42>(
        "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8",
    );

    assert_case(Some(&[]), &ikm, &[], &expected_prk, &expected_okm);
}

/// Check both published stages without collapsing their boundary.
fn assert_case<const N: usize>(
    salt: Option<&[u8]>,
    ikm: &[u8],
    info: &[u8],
    expected_prk: &[u8; 32],
    expected_okm: &[u8; N],
) {
    let prk = extract(salt, ikm).expect("the RFC fixture is within HMAC limits");
    assert_eq!(prk.expose_secret(), expected_prk);

    let mut output = [0_u8; N];
    prk.expand(info, &mut output)
        .expect("the RFC fixture is within HKDF limits");
    assert_eq!(&output, expected_okm);
}

fn fixture_index(index: usize) -> u8 {
    u8::try_from(index).expect("every RFC 5869 SHA-256 fixture index is at most 79")
}
