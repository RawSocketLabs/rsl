//! Published RFC 7748 X448 vectors, Wycheproof cases, and boundary evidence.

use rsl_crypto::{
    CryptoError,
    agreement::{
        KeyAgreement,
        x448::{X448, X448PrivateKey, X448PublicKey},
    },
};

use crate::wycheproof_fixtures::CASES;

fn decode(hex: &str) -> [u8; 56] {
    assert_eq!(hex.len(), 112, "an X448 fixture has 56 bytes");
    core::array::from_fn(|i| {
        u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("fixture is hex")
    })
}

fn decode_vec(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("fixture is hex"))
        .collect()
}

fn agree(scalar: &str, u: &str) -> [u8; 56] {
    X448::agree(
        &X448PrivateKey::new(decode(scalar)),
        &X448PublicKey::new(decode(u)),
    )
    .expect("published inputs give a nonzero result")
    .into_inner()
}

/// Published evidence: RFC 7748 §5.2's two direct X448 vectors.
#[test]
fn rfc_7748_direct_vectors() {
    assert_eq!(
        agree(
            "3d262fddf9ec8e88495266fea19a34d28882acef045104d0d1aae121700a779c984c24f8cdd78fbff44943eba368f54b29259a4f1c600ad3",
            "06fce640fa3487bfda5f6cf2d5263f8aad88334cbd07437f020f08f9814dc031ddbdc38c19c6da2583fa5429db94ada18aa7a7fb4ef8a086"
        ),
        decode(
            "ce3e4ff95a60dc6697da1db1d85e6afbdf79b50a2412d7546d5f239fe14fbaadeb445fc66a01b0779d98223961111e21766282f73dd96b6f"
        )
    );
    assert_eq!(
        agree(
            "203d494428b8399352665ddca42f9de8fef600908e0d461cb021f8c538345dd77c3e4806e25f46d3315c44e0a5b4371282dd2c8d5be3095f",
            "0fbcc2f993cd56d3305b0b7d9e55d4c1a8fb5dbb52f8e9a1e9b6201b165d015894e56c4d3570bee52fe205e28a78b91cdfbde71ce8d157db"
        ),
        decode(
            "884a02576239ff7a2f2f63b2db6a9ff37047ac13568e1e30fe63c4a7ad1b3ee3a5700df34321d62077e63633c575c1c954514e99da7c179d"
        )
    );
}

fn iterate(count: usize) -> [u8; 56] {
    let mut k = [0_u8; 56];
    k[0] = 5;
    let mut u = k;
    for _ in 0..count {
        let next = X448::agree(&X448PrivateKey::new(k), &X448PublicKey::new(u))
            .expect("iterated values are nonzero")
            .into_inner();
        u = k;
        k = next;
    }
    k
}

/// Published evidence: RFC 7748 §5.2 iterated results after one and 1,000 applications.
#[test]
fn rfc_7748_iterated_vectors() {
    assert_eq!(
        iterate(1),
        decode(
            "3f482c8a9f19b01e6c46ee9711d9dc14fd4bf67af30765c2ae2b846a4d23a8cd0db897086239492caf350b51f833868b9bc2b3bca9cf4113"
        )
    );
    assert_eq!(
        iterate(1_000),
        decode(
            "aa3b4749d55b9daf1e5b00288826c467274ce3ebbdd5c17b975e09d4af6c67cf10d087202db88286e2b79fceea3ec353ef54faa26e219f38"
        )
    );
}

/// Published evidence: RFC 7748 §5.2 after 1,000,000 applications. Ignored by default because
/// the deliberately unoptimized ladder takes minutes; run with `--ignored` to check it.
#[test]
#[ignore = "one million ladder iterations; run explicitly"]
fn rfc_7748_one_million_iterations() {
    assert_eq!(
        iterate(1_000_000),
        decode(
            "077f453681caca3693198420bbe515cae0002472519b3e67661a7e89cab94695c8f4bcd66e61b9b9c946da8d524de3d69bd9d9d66b997e37"
        )
    );
}

/// Published evidence: RFC 7748 §6.2's complete Alice/Bob exchange.
#[test]
fn rfc_7748_section_6_2_exchange() {
    let alice = X448PrivateKey::new(decode(
        "9a8f4925d1519f5775cf46b04b5800d4ee9ee8bae8bc5565d498c28dd9c9baf574a9419744897391006382a6f127ab1d9ac2d8c0a598726b",
    ));
    let bob = X448PrivateKey::new(decode(
        "1c306a7ac2a0e2e0990b294470cba339e6453772b075811d8fad0d1d6927c120bb5ee8972b0d3e21374c9c921b09d1b0366f10b65173992d",
    ));
    assert_eq!(
        X448::public_key(&alice).into_bytes(),
        decode(
            "9b08f7cc31b7e3e67d22d5aea121074a273bd2b83de09c63faa73d2c22c5d9bbc836647241d953d40c5b12da88120d53177f80e532c41fa0"
        )
    );
    assert_eq!(
        X448::public_key(&bob).into_bytes(),
        decode(
            "3eb7a829b0cd20f5bcfc0b599b6feccf6da4627107bdb0d4f345b43027d8b972fc3e34fb4232a13ca706dcb57aec3dae07bdc1c67bf33609"
        )
    );
    let expected = decode(
        "07fff4181ac6cc95ec1c16a94a0f74d12da232ce40a77552281d282bb60c0b56fd2464c335543936521c24403085d59a449a5037514a879d",
    );
    assert_eq!(
        X448::agree(&alice, &X448::public_key(&bob))
            .unwrap()
            .expose_secret(),
        &expected
    );
    assert_eq!(
        <X448 as KeyAgreement>::agree(&bob, &X448::public_key(&alice))
            .unwrap()
            .expose_secret(),
        &expected
    );
}

/// Published evidence: all 510 Wycheproof `x448` cases. `valid` and `acceptable` cases must
/// reproduce the shared secret unless it is all-zero, which this API rejects; over-long public
/// keys are unrepresentable.
#[test]
fn wycheproof_results_are_reproduced() {
    let mut reproduced = 0;
    let mut rejected_zero = 0;
    let mut unrepresentable = 0;
    for case in &CASES {
        if case.public.len() != 112 {
            assert_eq!(case.result, "invalid", "tcId {}", case.tc_id);
            assert!(X448PublicKey::try_from(decode_vec(case.public).as_slice()).is_err());
            unrepresentable += 1;
            continue;
        }
        let private = X448PrivateKey::new(decode(case.private));
        let outcome = X448::agree(&private, &X448PublicKey::new(decode(case.public)));
        let expected = decode(case.shared);
        if expected == [0; 56] {
            assert!(
                case.flags.contains("ZeroSharedSecret"),
                "tcId {}",
                case.tc_id
            );
            assert_eq!(
                outcome.err(),
                Some(CryptoError::InvalidPublicKey),
                "tcId {}",
                case.tc_id
            );
            rejected_zero += 1;
        } else {
            assert_eq!(
                outcome.unwrap().expose_secret(),
                &expected,
                "tcId {} ({}; {})",
                case.tc_id,
                case.comment,
                case.flags
            );
            reproduced += 1;
        }
    }
    assert_eq!(reproduced + rejected_zero + unrepresentable, 510);
    assert_eq!(unrepresentable, 12);
    assert_eq!(
        rejected_zero, 11,
        "Wycheproof publishes eleven all-zero shared secrets"
    );
}

/// Standard-derived evidence: exact wire length, all-zero rejection, and redaction.
#[test]
fn wire_length_zero_rejection_and_redaction() {
    assert_eq!(
        X448PublicKey::try_from([0_u8; 32].as_slice()),
        Err(CryptoError::InvalidLength {
            name: "X448 public key",
            expected: 56,
            actual: 32,
        })
    );
    let private = X448PrivateKey::new([0x42; 56]);
    assert_eq!(
        X448::agree(&private, &X448PublicKey::new([0; 56])).err(),
        Some(CryptoError::InvalidPublicKey)
    );
    assert_eq!(format!("{private:?}"), "X448PrivateKey([REDACTED])");
}
