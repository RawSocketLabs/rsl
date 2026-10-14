//! Validation, range, ownership, and generic-contract evidence.

use rsl_crypto::{
    CryptoError, RandomSource, Result,
    agreement::{
        KeyAgreement,
        ecdh_p384::{EcdhP384, EcdhP384PrivateKey, EcdhP384PublicKey},
    },
};

use crate::{cavp_pkv_fixtures::CASES, support};

/// SP 800-186 §3.2.1.3 group order `n`, big-endian.
const ORDER: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC7634D81F4372DDF581A0DB248B0A77AECEC196ACCC52973";

/// Published evidence: every CAVP PKV P-384 verdict is reproduced by public-key validation.
#[test]
fn cavp_public_key_validation_verdicts_are_reproduced() {
    for case in &CASES {
        let parsed = if case.x.len() == 96 && case.y.len() == 96 {
            EcdhP384PublicKey::from_bytes(support::uncompressed(case.x, case.y))
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
    let valid = EcdhP384::public_key(&EcdhP384PrivateKey::from_bytes([0x42; 48]).unwrap());
    let mut compressed_prefix = valid.into_bytes();
    compressed_prefix[0] = 0x02;
    assert_eq!(
        EcdhP384PublicKey::from_bytes(compressed_prefix),
        Err(CryptoError::InvalidPublicKey)
    );

    let mut x_is_p = valid.into_bytes();
    x_is_p[1..49].copy_from_slice(&support::decode::<48>(
        "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFFFF0000000000000000FFFFFFFF",
    ));
    assert_eq!(
        EcdhP384PublicKey::from_bytes(x_is_p),
        Err(CryptoError::InvalidPublicKey)
    );

    let mut off_curve = valid.into_bytes();
    off_curve[64] ^= 0x01;
    assert_eq!(
        EcdhP384PublicKey::from_bytes(off_curve),
        Err(CryptoError::InvalidPublicKey)
    );

    assert_eq!(
        EcdhP384PublicKey::try_from([0_u8; 49].as_slice()),
        Err(CryptoError::InvalidLength {
            name: "ECDH P-384 public key",
            expected: 97,
            actual: 49,
        })
    );
    assert!(EcdhP384PublicKey::try_from(valid.as_bytes().as_slice()).is_ok());
}

/// Standard-derived evidence: private scalars must lie in `[1, n-1]`.
#[test]
fn private_scalars_outside_one_through_n_minus_one_are_rejected() {
    let n: [u8; 48] = support::decode(ORDER);
    let mut n_minus_one = n;
    n_minus_one[47] -= 1;
    let mut one = [0_u8; 48];
    one[47] = 1;

    assert!(EcdhP384PrivateKey::from_bytes([0; 48]).is_err());
    assert!(EcdhP384PrivateKey::from_bytes(n).is_err());
    assert!(EcdhP384PrivateKey::from_bytes([0xff; 48]).is_err());
    assert!(EcdhP384PrivateKey::from_bytes(one).is_ok());
    assert!(EcdhP384PrivateKey::from_bytes(n_minus_one).is_ok());
    assert!(matches!(
        EcdhP384PrivateKey::from_bytes(n),
        Err(CryptoError::InvalidKey)
    ));
}

/// Standard-derived evidence: `[n-1]G = -G` shares `x` with `G`, so `Z` matches for `d = 1`.
#[test]
fn negated_generator_has_the_generator_x_coordinate() {
    let n: [u8; 48] = support::decode(ORDER);
    let mut n_minus_one = n;
    n_minus_one[47] -= 1;
    let mut one = [0_u8; 48];
    one[47] = 1;
    let d_one = EcdhP384PrivateKey::from_bytes(one).unwrap();
    let d_max = EcdhP384PrivateKey::from_bytes(n_minus_one).unwrap();
    let g = EcdhP384::public_key(&d_one);
    let minus_g = EcdhP384::public_key(&d_max);

    assert_eq!(g.as_bytes()[1..49], minus_g.as_bytes()[1..49]);
    assert_ne!(g.as_bytes()[49..], minus_g.as_bytes()[49..]);
    assert_eq!(
        EcdhP384::agree(&d_one, &minus_g).unwrap().expose_secret(),
        &g.as_bytes()[1..49]
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
    let key = EcdhP384PrivateKey::generate(&mut source).unwrap();
    let mut expected = [0x10_u8; 48];
    expected[47] = 0x11;
    let from_bytes = EcdhP384PrivateKey::from_bytes(expected).unwrap();
    assert_eq!(
        EcdhP384::public_key(&key).as_bytes(),
        EcdhP384::public_key(&from_bytes).as_bytes()
    );

    let mut exhausted = CountingSource { fills: vec![] };
    assert_eq!(
        EcdhP384PrivateKey::generate(&mut exhausted).err(),
        Some(CryptoError::EntropyUnavailable)
    );
}

/// Regression evidence: the generic contract dispatches to the inherent implementation.
#[test]
fn generic_key_agreement_dispatch_matches_the_inherent_path() {
    let alice = EcdhP384PrivateKey::from_bytes([0x11; 48]).unwrap();
    let bob = EcdhP384PrivateKey::from_bytes([0x22; 48]).unwrap();
    let bob_public = <EcdhP384 as KeyAgreement>::public_key(&bob);
    let generic = <EcdhP384 as KeyAgreement>::agree(&alice, &bob_public).unwrap();
    let inherent = EcdhP384::agree(&alice, &EcdhP384::public_key(&bob)).unwrap();
    assert_eq!(generic.expose_secret(), inherent.expose_secret());
    assert_eq!(format!("{alice:?}"), "EcdhP384PrivateKey([REDACTED])");
}
