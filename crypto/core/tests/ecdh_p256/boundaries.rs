//! Validation, range, ownership, and generic-contract evidence.

use rsl_crypto::{
    CryptoError, RandomSource, Result,
    agreement::{
        KeyAgreement,
        ecdh_p256::{EcdhP256, EcdhP256PrivateKey, EcdhP256PublicKey},
    },
};

use crate::{cavp_pkv_fixtures::CASES, support};

/// SP 800-186 §3.2.1.3 group order `n`, big-endian.
const ORDER: &str = "FFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551";

/// Published evidence: every CAVP PKV P-256 verdict is reproduced by public-key validation.
#[test]
fn cavp_public_key_validation_verdicts_are_reproduced() {
    for case in &CASES {
        let parsed = if case.x.len() == 64 && case.y.len() == 64 {
            EcdhP256PublicKey::from_bytes(support::uncompressed(case.x, case.y))
        } else {
            // NIST prints an out-of-range coordinate with 65 digits; it cannot be a 32-byte
            // field element, so the wire form itself is unrepresentable.
            Err(CryptoError::InvalidPublicKey)
        };
        let accepted = parsed.is_ok();
        let expected = case.verdict.starts_with('P');
        assert_eq!(accepted, expected, "x={} verdict {}", case.x, case.verdict);
    }
}

/// Standard-derived evidence: SP 800-56A §5.6.2.3.3 rejections are distinguishable by cause.
#[test]
fn prefix_range_and_curve_equation_failures_are_rejected() {
    let valid = EcdhP256::public_key(&EcdhP256PrivateKey::from_bytes([0x42; 32]).unwrap());
    let mut compressed_prefix = valid.into_bytes();
    compressed_prefix[0] = 0x02;
    assert_eq!(
        EcdhP256PublicKey::from_bytes(compressed_prefix),
        Err(CryptoError::InvalidPublicKey)
    );

    let mut x_is_p = valid.into_bytes();
    x_is_p[1..33].copy_from_slice(&support::decode::<32>(
        "FFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF",
    ));
    assert_eq!(
        EcdhP256PublicKey::from_bytes(x_is_p),
        Err(CryptoError::InvalidPublicKey)
    );

    let mut off_curve = valid.into_bytes();
    off_curve[64] ^= 0x01;
    assert_eq!(
        EcdhP256PublicKey::from_bytes(off_curve),
        Err(CryptoError::InvalidPublicKey)
    );

    assert_eq!(
        EcdhP256PublicKey::try_from([0_u8; 33].as_slice()),
        Err(CryptoError::InvalidLength {
            name: "ECDH P-256 public key",
            expected: 65,
            actual: 33,
        })
    );
    assert!(EcdhP256PublicKey::try_from(valid.as_bytes().as_slice()).is_ok());
}

/// Standard-derived evidence: private scalars must lie in `[1, n-1]`.
#[test]
fn private_scalars_outside_one_through_n_minus_one_are_rejected() {
    let n: [u8; 32] = support::decode(ORDER);
    let mut n_minus_one = n;
    n_minus_one[31] -= 1;
    let mut one = [0_u8; 32];
    one[31] = 1;

    assert!(EcdhP256PrivateKey::from_bytes([0; 32]).is_err());
    assert!(EcdhP256PrivateKey::from_bytes(n).is_err());
    assert!(EcdhP256PrivateKey::from_bytes([0xff; 32]).is_err());
    assert!(EcdhP256PrivateKey::from_bytes(one).is_ok());
    assert!(EcdhP256PrivateKey::from_bytes(n_minus_one).is_ok());
    assert!(matches!(
        EcdhP256PrivateKey::from_bytes(n),
        Err(CryptoError::InvalidKey)
    ));
}

/// Standard-derived evidence: `[n-1]G = -G` shares `x` with `G`, so `Z` matches for `d = 1`.
#[test]
fn negated_generator_has_the_generator_x_coordinate() {
    let n: [u8; 32] = support::decode(ORDER);
    let mut n_minus_one = n;
    n_minus_one[31] -= 1;
    let mut one = [0_u8; 32];
    one[31] = 1;
    let d_one = EcdhP256PrivateKey::from_bytes(one).unwrap();
    let d_max = EcdhP256PrivateKey::from_bytes(n_minus_one).unwrap();
    let g = EcdhP256::public_key(&d_one);
    let minus_g = EcdhP256::public_key(&d_max);

    assert_eq!(g.as_bytes()[1..33], minus_g.as_bytes()[1..33]);
    assert_ne!(g.as_bytes()[33..], minus_g.as_bytes()[33..]);
    assert_eq!(
        EcdhP256::agree(&d_one, &minus_g).unwrap().expose_secret(),
        &g.as_bytes()[1..33]
    );
}

struct CountingSource {
    fills: Vec<u8>,
}

impl RandomSource for CountingSource {
    fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
        let value = self.fills.pop().ok_or(CryptoError::EntropyUnavailable)?;
        output.fill(value);
        Ok(())
    }
}

/// Standard-derived evidence: candidate testing skips `c > n - 2` and returns `d = c + 1`.
#[test]
fn generation_retries_out_of_range_candidates_then_adds_one() {
    let mut source = CountingSource {
        fills: vec![0x10, 0xff],
    };
    let key = EcdhP256PrivateKey::generate(&mut source).unwrap();
    let mut expected = [0x10_u8; 32];
    expected[31] = 0x11;
    let from_bytes = EcdhP256PrivateKey::from_bytes(expected).unwrap();
    assert_eq!(
        EcdhP256::public_key(&key).as_bytes(),
        EcdhP256::public_key(&from_bytes).as_bytes()
    );

    let mut exhausted = CountingSource { fills: vec![] };
    assert_eq!(
        EcdhP256PrivateKey::generate(&mut exhausted).err(),
        Some(CryptoError::EntropyUnavailable)
    );
}

/// Regression evidence: the generic contract dispatches to the inherent implementation.
#[test]
fn generic_key_agreement_dispatch_matches_the_inherent_path() {
    let alice = EcdhP256PrivateKey::from_bytes([0x11; 32]).unwrap();
    let bob = EcdhP256PrivateKey::from_bytes([0x22; 32]).unwrap();
    let bob_public = <EcdhP256 as KeyAgreement>::public_key(&bob);
    let generic = <EcdhP256 as KeyAgreement>::agree(&alice, &bob_public).unwrap();
    let inherent = EcdhP256::agree(&alice, &EcdhP256::public_key(&bob)).unwrap();
    assert_eq!(generic.expose_secret(), inherent.expose_secret());
    assert_eq!(format!("{alice:?}"), "EcdhP256PrivateKey([REDACTED])");
}
