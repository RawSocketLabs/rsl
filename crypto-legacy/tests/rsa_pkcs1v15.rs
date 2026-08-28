//! Published signature/encryption, malformed-input, boundary, and classification evidence.

use rsl_crypto_legacy::{
    CryptoError, SecurityStatus,
    rsa::{
        RSA_PRIMITIVE_SECURITY_STATUS, RsaPrivateKey, RsaPublicKey,
        pkcs1v15::{
            Pkcs1v15PrivateOperations, Pkcs1v15PublicOperations, RSAES_SECURITY_STATUS,
            RSASSA_SHA1_SECURITY_STATUS, RSASSA_SHA256_SECURITY_STATUS, RsaPkcs1v15Ciphertext,
            RsaPkcs1v15Signature,
        },
    },
};

const NIST_MODULUS: &str = concat!(
    "cea80475324c1dc8347827818da58bac069d3419c614a6ea1ac6a3b510dcd72",
    "cc516954905e9fef908d45e13006adf27d467a7d83c111d1a5df15ef293771ae",
    "fb920032a5bb989f8e4f5e1b05093d3f130f984c07a772a3683f4dc6fb28a968",
    "15b32123ccdd13954f19d5b8b24a103e771a34c328755c65ed64e1924ffd04d30",
    "b2142cc262f6e0048fef6dbc652f21479ea1c4b1d66d28f4d46ef7185e390cbf",
    "a2e02380582f3188bb94ebbf05d31487a09aff01fcbb4cd4bfd1f0a833b38c11",
    "813c84360bb53c7d4481031c40bad8713bb6b835cb08098ed15ba31ee4ba728a8",
    "c8e10f7294e1b4163b7aee57277bfd881a6f9d43e02c6925aa3a043fb7fb78d",
);
const NIST_PRIVATE_EXPONENT: &str = concat!(
    "0997634c477c1a039d44c810b2aaa3c7862b0b88d3708272e1e15f66fc938970",
    "9f8a11f3ea6a5af7effa2d01c189c50f0d5bcbe3fa272e56cfc4a4e1d388a9d",
    "cd65df8628902556c8b6bb6a641709b5a35dd2622c73d4640bfa1359d0e76e1",
    "f219f8e33eb9bd0b59ec198eb2fccaae0346bd8b401e12e3c67cb629569c185a",
    "2e0f35a2f741644c1cca5ebb139d77a89a2953fc5e30048c0e619f07c8d21d1",
    "e56b8af07193d0fdf3f49cd49f2ef3138b5138862f1470bd2d16e34a2b9e777",
    "7a6c8c8d4cb94b4e8b5d616cd5393753e7b0f31cc7da559ba8e98d888914e33",
    "4773baf498ad88d9631eb5fe32e53a4145bf0ba548bf2b0a50c63f67b14e398a",
    "34b0d",
);
const NIST_SHA256_MESSAGE: &str = concat!(
    "5af283b1b76ab2a695d794c23b35ca7371fc779e92ebf589e304c7f923d8cf97",
    "6304c19818fcd89d6f07c8d8e08bf371068bdf28ae6ee83b2e02328af8c0e2f",
    "96e528e16f852f1fc5455e4772e288a68f159ca6bdcf902b858a1f94789b3163",
    "823e2d0717ff56689eec7d0e54d93f520d96e1eb04515abc70ae90578ff38d31b",
);
const NIST_SHA256_SIGNATURE: &str = concat!(
    "6b8be97d9e518a2ede746ff4a7d91a84a1fc665b52f154a927650db6e7348c69",
    "f8c8881f7bcf9b1a6d3366eed30c3aed4e93c203c43f5528a45de791895747ad",
    "e9c5fa5eee81427edee02082147aa311712a6ad5fb1732e93b3d6cd23ffd46a0b",
    "3caf62a8b69957cc68ae39f9993c1a779599cdda949bdaababb77f248fcfeaa44",
    "059be5459fb9b899278e929528ee130facd53372ecbc42f3e8de2998425860406",
    "440f248d817432de687112e504d734028e6c5620fa282ca07647006cf0a2ff83e",
    "19a916554cc61810c2e855305db4e5cf893a6a96767365794556ff033359084d7e",
    "38a8456e68e21155b76151314a29875feee09557161cbc654541e89e42",
);

const WYCHEPROOF_MODULUS: &str = concat!(
    "00b3510a2bcd4ce644c5b594ae5059e12b2f054b658d5da5959a2fdf1871b808",
    "bc3df3e628d2792e51aad5c124b43bda453dca5cde4bcf28e7bd4effba0cb4b7",
    "42bbb6d5a013cb63d1aa3a89e02627ef5398b52c0cfd97d208abeb8d7c9bce0b",
    "beb019a86ddb589beb29a5b74bf861075c677c81d430f030c265247af9d3c9140",
    "ccb65309d07e0adc1efd15cf17e7b055d7da3868e4648cc3a180f0ee7f8e1e7b",
    "18098a3391b4ce7161e98d57af8a947e201a463e2d6bbca8059e5706e9dfed8f",
    "4856465ffa712ed1aa18e888d12dc6aa09ce95ecfca83cc5b0b15db09c8647f5",
    "d524c0f2e7620a3416b9623cadc0f097af573261c98c8400aa12af38e43cad84d",
);
const WYCHEPROOF_PRIVATE_EXPONENT: &str = concat!(
    "1a502d0eea6c7b69e21d5839101f705456ed0ef852fb47fe21071f54c5f33c8c",
    "eb066c62d727e32d26c58137329f89d3195325b795264c195d85472f7507dbd0",
    "961d2951f935a26b34f0ac24d15490e1128a9b7138915bc7dbfa8fe396357131",
    "c543ae9c98507368d9ceb08c1c6198a3eda7aea185a0e976cd42c22d00f003d9",
    "f19d96ea4c9afcbfe1441ccc802cfb0689f59d804c6a4e4f404c15174745ed6c",
    "b8bc88ef0b33ba0d2a80e35e43bc90f350052e72016e75b00d357a381c9c0d4",
    "67069ca660887c987766349fcc43460b4aa516bce079edd87ba164307b752c277",
    "ed9528ad3ba0bf1877349ed3b7966a6c240110409bf4d0fade0c68fdadd847fd",
);
const WYCHEPROOF_VALID_CIPHERTEXT_TC3: &str = concat!(
    "4501b4d669e01b9ef2dc800aa1b06d49196f5a09fe8fbcd037323c60eaf027bf",
    "b98432be4e4a26c567ffec718bcbea977dd26812fa071c33808b4d5ebb742d987",
    "9806094b6fbeea63d25ea3141733b60e31c6912106e1b758a7fe0014f075193f",
    "aa8b4622bfd5d3013f0a32190a95de61a3604711bc62945f95a6522bd4dfed0a",
    "994ef185b28c281f7b5e4c8ed41176d12d9fc1b837e6a0111d0132d08a6d6f0",
    "580de0c9eed8ed105531799482d1e466c68c23b0c222af7fc12ac279bc4ff57e",
    "7b4586d209371b38c4c1035edd418dc5f960441cb21ea2bedbfea86de0d7861e",
    "81021b650a1de51002c315f1e7c12debe4dcebf790caaa54a2f26b149cf9e77d",
);
const WYCHEPROOF_INVALID_CIPHERTEXT_TC14: &str = concat!(
    "3307264f64d4ca8b62c4e7da4cac117262e5d3a3dbc19a529ac5167c1987bce5",
    "6e358726d0ecfc6cb591a12bd5f7531cd2249439254c366ad3cb7a608f845e1e",
    "ca931018295208ba5c6198027b22191224c4568856ab331e2acf530fc434870865",
    "d3321ac90327a8c61f27cac9859dac8e3c38d8453349d2ef8e4a7e8011f6badd",
    "1530eae710e0c60d35905f20d7a2d118e7ce18ebb220f04b4089778cbf091bcb",
    "3e02aca83b4b9ba5319c3069188c7b00c7d32ebe1dd6e6535b5f667ce972f00b",
    "a773d4cf6a556ccf65bacc1eca2312881caf6a89ff5d83960846a5d9dd31477d",
    "cc9ee4ae50ab0cb2e574a685bd9d7b7a74c7ca9876f08fd64d1d5f196786be",
);

#[test]
fn nist_cavp_sha256_signature_is_generated_and_verified_exactly() {
    let modulus = decode_hex(NIST_MODULUS);
    let public = RsaPublicKey::from_components(&modulus, decode_hex("260445")).unwrap();
    let private =
        RsaPrivateKey::from_components(&modulus, decode_hex(NIST_PRIVATE_EXPONENT)).unwrap();
    let message = decode_hex(NIST_SHA256_MESSAGE);
    let expected = decode_hex(NIST_SHA256_SIGNATURE);

    let generated = private.sign_pkcs1v15_sha256(&message).unwrap();
    assert_eq!(generated.as_bytes(), expected);
    public.verify_pkcs1v15_sha256(&message, &generated).unwrap();

    let mut changed_message = message;
    changed_message[0] ^= 1;
    assert_eq!(
        public.verify_pkcs1v15_sha256(&changed_message, &generated),
        Err(CryptoError::InvalidSignature),
    );
}

#[test]
fn sha1_profile_round_trips_but_remains_machine_readably_broken() {
    let modulus = decode_hex(NIST_MODULUS);
    let public = RsaPublicKey::from_components(&modulus, decode_hex("260445")).unwrap();
    let private =
        RsaPrivateKey::from_components(&modulus, decode_hex(NIST_PRIVATE_EXPONENT)).unwrap();
    let signature = private
        .sign_pkcs1v15_sha1(b"historical SHA-1 profile")
        .unwrap();

    public
        .verify_pkcs1v15_sha1(b"historical SHA-1 profile", &signature)
        .unwrap();
    assert_eq!(RSASSA_SHA1_SECURITY_STATUS, SecurityStatus::Broken);
}

#[test]
fn wycheproof_accepts_valid_encoding_and_uniformly_rejects_short_padding() {
    let private = RsaPrivateKey::from_components(
        decode_hex(WYCHEPROOF_MODULUS),
        decode_hex(WYCHEPROOF_PRIVATE_EXPONENT),
    )
    .unwrap();
    let valid = RsaPkcs1v15Ciphertext::from_bytes(decode_hex(WYCHEPROOF_VALID_CIPHERTEXT_TC3));
    assert_eq!(private.decrypt_pkcs1v15(&valid).unwrap(), b"Test");

    // Wycheproof tcId 14 places a zero at padding-string byte seven, one byte before the minimum.
    let invalid = RsaPkcs1v15Ciphertext::from_bytes(decode_hex(WYCHEPROOF_INVALID_CIPHERTEXT_TC14));
    assert_eq!(
        private.decrypt_pkcs1v15(&invalid),
        Err(CryptoError::AuthenticationFailed),
    );

    let wrong_length = RsaPkcs1v15Ciphertext::from_bytes(vec![0; private.modulus_len() - 1]);
    assert_eq!(
        private.decrypt_pkcs1v15(&wrong_length),
        Err(CryptoError::AuthenticationFailed),
    );
}

#[test]
fn imported_component_and_encoding_boundaries_reject_before_use() {
    assert!(matches!(
        RsaPublicKey::from_components([0x80, 0], [3]),
        Err(CryptoError::InvalidPublicKey),
    ));
    assert!(matches!(
        RsaPrivateKey::from_components([0x80, 0, 1], [1]),
        Err(CryptoError::InvalidKey),
    ));

    let public =
        RsaPublicKey::from_components([0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], [3]).unwrap();
    let malformed_signature = RsaPkcs1v15Signature::from_bytes(vec![0; public.modulus_len() - 1]);
    assert_eq!(
        public.verify_pkcs1v15_sha1(b"x", &malformed_signature),
        Err(CryptoError::InvalidSignature),
    );
    assert!(format!("{public:?}").contains("components: \"[OMITTED]\""));
}

#[test]
fn every_rsa_layer_exposes_its_non_recommended_status() {
    assert_eq!(
        RSA_PRIMITIVE_SECURITY_STATUS,
        SecurityStatus::EducationalOnly
    );
    assert_eq!(RSAES_SECURITY_STATUS, SecurityStatus::Broken);
    assert_eq!(RSASSA_SHA1_SECURITY_STATUS, SecurityStatus::Broken);
    assert_eq!(RSASSA_SHA256_SECURITY_STATUS, SecurityStatus::Legacy);
}

fn decode_hex(encoded: &str) -> Vec<u8> {
    assert_eq!(encoded.len() % 2, 0, "fixture contains complete bytes");
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = core::str::from_utf8(pair).expect("fixture is ASCII");
            u8::from_str_radix(pair, 16).expect("fixture is hexadecimal")
        })
        .collect()
}
