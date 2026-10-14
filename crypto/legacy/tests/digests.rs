//! Public known-answer, boundary, streaming, classification, and differential digest evidence.

use md5::{Digest as _, Md5 as ReferenceMd5};
use rsl_crypto_legacy::{
    SecurityStatus,
    digest::{
        md5::{Md5, SECURITY_STATUS as MD5_STATUS},
        sha1::{SECURITY_STATUS as SHA1_STATUS, Sha1},
    },
};
use sha1::Sha1 as ReferenceSha1;

#[test]
fn sha1_fips_examples() {
    assert_eq!(
        Sha1::digest("abc").unwrap().into_bytes(),
        hex("a9993e364706816aba3e25717850c26c9cd0d89d")
    );
    assert_eq!(
        Sha1::digest("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
            .unwrap()
            .into_bytes(),
        hex("84983e441c3bd26ebaae4aa1f95129e5e54670f1")
    );
}

#[test]
fn md5_rfc_1321_suite() {
    let cases = [
        ("", "d41d8cd98f00b204e9800998ecf8427e"),
        ("a", "0cc175b9c0f1b6a831c399e269772661"),
        ("abc", "900150983cd24fb0d6963f7d28e17f72"),
        ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
        (
            "abcdefghijklmnopqrstuvwxyz",
            "c3fcd3d76192e4007dfb496cca67e13b",
        ),
        (
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            "d174ab98d277d9f5a5611c2c9f419d9f",
        ),
        (
            "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
            "57edf4a22be3c955ac49da2e2107b67a",
        ),
    ];
    for (message, expected) in cases {
        assert_eq!(Md5::digest(message).unwrap().into_bytes(), hex(expected));
    }
}

#[test]
fn fragmented_boundaries_match_one_shot_and_independent_oracles() {
    for length in [0_usize, 1, 55, 56, 63, 64, 65, 127, 128, 129, 1024] {
        let message: Vec<u8> = (0..length)
            .map(|index| index.to_le_bytes()[0].wrapping_mul(91).wrapping_add(7))
            .collect();

        let reference_sha1 = ReferenceSha1::digest(&message);
        let reference_md5 = ReferenceMd5::digest(&message);
        assert_eq!(
            Sha1::digest(&message).unwrap().as_ref(),
            reference_sha1.as_slice()
        );
        assert_eq!(
            Md5::digest(&message).unwrap().as_ref(),
            reference_md5.as_slice()
        );

        let mut sha1 = Sha1::new();
        let mut md5 = Md5::new();
        for part in message.chunks(13) {
            sha1.update(part).unwrap();
            md5.update(part).unwrap();
        }
        assert_eq!(sha1.finalize().as_ref(), reference_sha1.as_slice());
        assert_eq!(md5.finalize().as_ref(), reference_md5.as_slice());
    }
}

#[test]
fn neither_broken_digest_can_be_mistaken_for_recommended() {
    assert_eq!(SHA1_STATUS, SecurityStatus::Broken);
    assert_eq!(MD5_STATUS, SecurityStatus::Broken);
    assert_ne!(SHA1_STATUS, SecurityStatus::Recommended);
    assert_ne!(MD5_STATUS, SecurityStatus::Recommended);
}

fn hex<const N: usize>(input: &str) -> [u8; N] {
    assert_eq!(input.len(), N * 2);
    core::array::from_fn(|index| {
        u8::from_str_radix(&input[index * 2..index * 2 + 2], 16).expect("fixture contains hex")
    })
}
