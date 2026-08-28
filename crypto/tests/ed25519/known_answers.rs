//! Published pure-Ed25519 vectors from RFC 8032 §7.1.

use rsl_crypto::signature::ed25519::{Ed25519Signature, Ed25519SigningKey, Ed25519VerifyingKey};

use super::support::hex;

#[test]
fn rfc_8032_test_one_empty_message() {
    assert_vector(
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
        &[],
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
         5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
    );
}

#[test]
fn rfc_8032_test_two_one_byte_message() {
    assert_vector(
        "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
        "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
        &hex::<1>("72"),
        "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da\
         085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
    );
}

#[test]
fn rfc_8032_test_three_two_byte_message() {
    assert_vector(
        "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
        "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
        &hex::<2>("af82"),
        "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac\
         18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
    );
}

fn assert_vector(seed: &str, public: &str, message: &[u8], signature: &str) {
    let signing = Ed25519SigningKey::from_seed(hex(seed));
    let expected_public = hex(public);
    let expected_signature = hex(signature);

    assert_eq!(signing.verifying_key().into_bytes(), expected_public);
    assert_eq!(
        signing.sign(message).unwrap().into_bytes(),
        expected_signature
    );

    let verifying = Ed25519VerifyingKey::from_bytes(expected_public).unwrap();
    verifying
        .verify(message, &Ed25519Signature::from_bytes(expected_signature))
        .unwrap();
}

/// RFC 8032 §7.2 Ed25519ctx vectors `foo`, `bar`, `foo2`, and `foo3`.
#[test]
fn rfc_8032_ed25519ctx_vectors() {
    use rsl_crypto::signature::ed25519::Ed25519Context;

    let cases = [
        (
            "0305334e381af78f141cb666f6199f57bc3495335a256a95bd2a55bf546663f6",
            "dfc9425e4f968f7f0c29f0259cf5f9aed6851c2bb4ad8bfb860cfee0ab248292",
            "f726936d19c800494e3fdaff20b276a8",
            "666f6f",
            "55a4cc2f70a54e04288c5f4cd1e45a7bb520b36292911876cada7323198dd87a\
             8b36950b95130022907a7fb7c4e9b2d5f6cca685a587b4b21f4b888e4e7edb0d",
        ),
        (
            "0305334e381af78f141cb666f6199f57bc3495335a256a95bd2a55bf546663f6",
            "dfc9425e4f968f7f0c29f0259cf5f9aed6851c2bb4ad8bfb860cfee0ab248292",
            "f726936d19c800494e3fdaff20b276a8",
            "626172",
            "fc60d5872fc46b3aa69f8b5b4351d5808f92bcc044606db097abab6dbcb1aee3\
             216c48e8b3b66431b5b186d1d28f8ee15a5ca2df6668346291c2043d4eb3e90d",
        ),
        (
            "0305334e381af78f141cb666f6199f57bc3495335a256a95bd2a55bf546663f6",
            "dfc9425e4f968f7f0c29f0259cf5f9aed6851c2bb4ad8bfb860cfee0ab248292",
            "508e9e6882b979fea900f62adceaca35",
            "666f6f",
            "8b70c1cc8310e1de20ac53ce28ae6e7207f33c3295e03bb5c0732a1d20dc6490\
             8922a8b052cf99b7c4fe107a5abb5b2c4085ae75890d02df26269d8945f84b0b",
        ),
        (
            "ab9c2853ce297ddab85c993b3ae14bcad39b2c682beabc27d6d4eb20711d6560",
            "0f1d1274943b91415889152e893d80e93275a1fc0b65fd71b4b0dda10ad7d772",
            "f726936d19c800494e3fdaff20b276a8",
            "666f6f",
            "21655b5f1aa965996b3f97b3c849eafba922a0a62992f73b3d1b73106a84ad85\
             e9b86a7b6005ea868337ff2d20a7f5fbd4cd10b0be49a68da2b2e0dc0ad8960f",
        ),
    ];
    for (seed, public, message, context, signature) in cases {
        let signing = Ed25519SigningKey::from_seed(hex(seed));
        let message = hex::<16>(message);
        let context = Ed25519Context::new(&hex::<3>(context)).unwrap();
        let expected_public = hex(public);
        let expected_signature = hex(signature);

        assert_eq!(signing.verifying_key().into_bytes(), expected_public);
        assert_eq!(
            signing
                .sign_with_context(&context, message)
                .unwrap()
                .into_bytes(),
            expected_signature
        );
        Ed25519VerifyingKey::from_bytes(expected_public)
            .unwrap()
            .verify_with_context(
                &context,
                message,
                &Ed25519Signature::from_bytes(expected_signature),
            )
            .unwrap();
    }
}

/// RFC 8032 §7.3 Ed25519ph vector `abc` with the default empty context.
#[test]
fn rfc_8032_ed25519ph_vector_abc() {
    use rsl_crypto::digest::sha2::sha512::Sha512;

    let signing = Ed25519SigningKey::from_seed(hex(
        "833fe62409237b9d62ec77587520911e9a759cec1d19755b7da901b96dca3d42",
    ));
    let expected_public =
        hex::<32>("ec172b93ad5e563bf4932c70e1245034c35467ef2efd4d64ebf819683467e2bf");
    let expected_signature = hex::<64>(
        "98a70222f0b8121aa9d30f813d683f809e462b469c7ff87639499bb94e6dae41\
         31f85042463c2a355a2003d062adf5aaa10b8c61e636062aaad11c2a26083406",
    );
    let digest = Sha512::digest(b"abc").unwrap();

    assert_eq!(signing.verifying_key().into_bytes(), expected_public);
    assert_eq!(
        signing.sign_prehashed(&digest, None).unwrap().into_bytes(),
        expected_signature
    );
    Ed25519VerifyingKey::from_bytes(expected_public)
        .unwrap()
        .verify_prehashed(
            &digest,
            None,
            &Ed25519Signature::from_bytes(expected_signature),
        )
        .unwrap();
}
