//! NIST P-256 (secp256r1, prime256v1): the parameter set and its evidence.
//!
//! # What P-256 is
//!
//! P-256 is the short Weierstrass curve `y^2 = x^3 - 3x + b` over the prime field `GF(p)`,
//! `p = 2^256 - 2^224 + 2^192 + 2^96 - 1`, with a prime group order `n` and cofactor one. Its
//! domain parameters are published in NIST SP 800-186 §3.2.1.3. TLS 1.3 names the curve
//! `secp256r1` for key exchange and `ecdsa_secp256r1_sha256` for signatures; SSH names it
//! `ecdh-sha2-nistp256` and `ecdsa-sha2-nistp256`.
//!
//! The arithmetic lives in [`crate::curve::weierstrass`], generic over the [`Curve`] parameter
//! trait; this module supplies the constants as [`P256`] and the type aliases the ECDH and ECDSA
//! schemes use. See that module for the notation table and the algorithm walkthrough.
//!
//! # Evidence and security status
//!
//! The private tests here check the published hexadecimal forms of `p`, `n`, and `G`, the
//! generator's curve membership, `[n]G = O`, negation, and encoding boundaries. Public evidence
//! lives with the ECDH and ECDSA consumers. No side-channel or audit claim is made.

#![allow(rustdoc::private_intra_doc_links)]

use crate::curve::weierstrass::{self, Curve, arithmetic::Modulus};

/// Limbs in a P-256 element.
pub(crate) const LIMBS: usize = 4;
/// Bytes in a P-256 coordinate or scalar.
pub(crate) const ELEMENT_BYTES: usize = weierstrass::element_bytes::<LIMBS>();
/// Bytes in an uncompressed P-256 point.
pub(crate) const ENCODED_LEN: usize = weierstrass::encoded_point_bytes::<LIMBS>();

/// The SP 800-186 §3.2.1.3 parameter set.
#[derive(Clone, Copy)]
pub(crate) struct P256;

impl Curve<LIMBS> for P256 {
    /// `p = 2^256 - 2^224 + 2^192 + 2^96 - 1`.
    const FIELD: Modulus<LIMBS> = Modulus::new([
        0xffff_ffff_ffff_ffff,
        0x0000_0000_ffff_ffff,
        0x0000_0000_0000_0000,
        0xffff_ffff_0000_0001,
    ]);
    const ORDER: Modulus<LIMBS> = Modulus::new([
        0xf3b9_cac2_fc63_2551,
        0xbce6_faad_a717_9e84,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_0000_0000,
    ]);
    const FIELD_INVERSION_EXPONENT: [u64; LIMBS] = [
        0xffff_ffff_ffff_fffd,
        0x0000_0000_ffff_ffff,
        0x0000_0000_0000_0000,
        0xffff_ffff_0000_0001,
    ];
    const ORDER_INVERSION_EXPONENT: [u64; LIMBS] = [
        0xf3b9_cac2_fc63_254f,
        0xbce6_faad_a717_9e84,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_0000_0000,
    ];
    const B: [u64; LIMBS] = [
        0x3bce_3c3e_27d2_604b,
        0x651d_06b0_cc53_b0f6,
        0xb3eb_bd55_7698_86bc,
        0x5ac6_35d8_aa3a_93e7,
    ];
    const GENERATOR_X: [u64; LIMBS] = [
        0xf4a1_3945_d898_c296,
        0x7703_7d81_2deb_33a0,
        0xf8bc_e6e5_63a4_40f2,
        0x6b17_d1f2_e12c_4247,
    ];
    const GENERATOR_Y: [u64; LIMBS] = [
        0xcbb6_4068_37bf_51f5,
        0x2bce_3357_6b31_5ece,
        0x8ee7_eb4a_7c0f_9e16,
        0x4fe3_42e2_fe1a_7f9b,
    ];
    const NAME: &'static str = "P-256 scalar";
}

#[cfg(test)]
pub(crate) type FieldElement = weierstrass::field::FieldElement<P256, LIMBS>;
pub(crate) type Scalar = weierstrass::scalar::Scalar<P256, LIMBS>;
pub(crate) type ProjectivePoint = weierstrass::point::ProjectivePoint<P256, LIMBS>;
pub(crate) type AffinePoint = weierstrass::point::AffinePoint<P256, LIMBS>;

/// FIPS 186-5 A.2.2 candidate testing for a 32-byte P-256 private scalar.
pub(crate) fn generate_private_bytes<R: crate::random::RandomSource>(
    random: &mut R,
) -> crate::Result<[u8; ELEMENT_BYTES]> {
    let mut out = [0_u8; ELEMENT_BYTES];
    weierstrass::scalar::generate_private_bytes::<P256, LIMBS, R>(random, &mut out)?;
    Ok(out)
}

/// Current project lifecycle classification for the P-256 group arithmetic.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;

#[cfg(test)]
mod unit {
    use super::*;
    use crate::curve::weierstrass::arithmetic;

    fn n_bytes() -> [u8; 32] {
        let mut out = [0_u8; 32];
        arithmetic::write_be_bytes(&P256::ORDER.value, &mut out);
        out
    }

    fn generator_bytes() -> [u8; ENCODED_LEN] {
        let mut out = [0_u8; ENCODED_LEN];
        ProjectivePoint::generator()
            .to_affine()
            .expect("the generator is finite")
            .write_bytes(&mut out);
        out
    }

    /// Published evidence: SP 800-186 §3.2.1.3 hexadecimal forms of `p` and `n`.
    #[test]
    fn modulus_and_order_match_the_published_hexadecimal_forms() {
        let mut p = [0_u8; 32];
        arithmetic::write_be_bytes(&P256::FIELD.value, &mut p);
        assert_eq!(
            p,
            [
                0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
                0xff, 0xff, 0xff, 0xff,
            ]
        );
        assert_eq!(
            n_bytes(),
            [
                0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
                0xff, 0xff, 0xbc, 0xe6, 0xfa, 0xad, 0xa7, 0x17, 0x9e, 0x84, 0xf3, 0xb9, 0xca, 0xc2,
                0xfc, 0x63, 0x25, 0x51,
            ]
        );
    }

    #[test]
    fn canonical_decoding_rejects_p_and_n_and_accepts_one_below() {
        let mut p = [0_u8; 32];
        arithmetic::write_be_bytes(&P256::FIELD.value, &mut p);
        let mut p_minus_one = p;
        p_minus_one[31] -= 1;
        assert!(FieldElement::from_canonical_bytes(&p).is_none());
        assert!(FieldElement::from_canonical_bytes(&p_minus_one).is_some());
        let n = n_bytes();
        let mut n_minus_one = n;
        n_minus_one[31] -= 1;
        assert!(Scalar::from_nonzero_canonical_bytes(&n).is_none());
        assert!(Scalar::from_nonzero_canonical_bytes(&n_minus_one).is_some());
        assert!(Scalar::from_nonzero_canonical_bytes(&[0; 32]).is_none());
        assert!(Scalar::from_canonical_bytes(&[0; 32]).is_some());
    }

    #[test]
    fn generator_is_on_the_curve_round_trips_and_has_order_n() {
        let encoded = generator_bytes();
        assert_eq!(encoded[0], 0x04);
        let mut again = [0_u8; ENCODED_LEN];
        AffinePoint::from_bytes(&encoded)
            .expect("published generator is on the curve")
            .write_bytes(&mut again);
        assert_eq!(again, encoded);
        let g = ProjectivePoint::generator();
        assert!(g.multiply(&n_bytes()).is_identity());
        let mut n_minus_one = n_bytes();
        n_minus_one[31] -= 1;
        let mut negated = [0_u8; ENCODED_LEN];
        g.multiply(&n_minus_one)
            .to_affine()
            .unwrap()
            .write_bytes(&mut negated);
        assert_eq!(negated[1..33], encoded[1..33], "same x");
        assert_ne!(negated[33..], encoded[33..], "negated y");
        assert!(g.add(&g.multiply(&n_minus_one)).is_identity());
        let mut two = [0_u8; 32];
        two[31] = 2;
        let mut doubled = [0_u8; ENCODED_LEN];
        g.double().to_affine().unwrap().write_bytes(&mut doubled);
        let mut times_two = [0_u8; ENCODED_LEN];
        g.multiply(&two)
            .to_affine()
            .unwrap()
            .write_bytes(&mut times_two);
        assert_eq!(doubled, times_two);
    }

    #[test]
    fn decoding_rejects_bad_prefix_out_of_range_off_curve_and_wrong_length() {
        let generator = generator_bytes();
        let mut encoded = generator;
        encoded[0] = 0x02;
        assert!(AffinePoint::from_bytes(&encoded).is_none());
        let mut off_curve = generator;
        off_curve[64] ^= 1;
        assert!(AffinePoint::from_bytes(&off_curve).is_none());
        let mut out_of_range = generator;
        out_of_range[1..33].copy_from_slice(&[0xff; 32]);
        assert!(AffinePoint::from_bytes(&out_of_range).is_none());
        assert!(AffinePoint::from_bytes(&generator[..64]).is_none());
    }

    #[test]
    fn scalar_arithmetic_wraps_and_inverts() {
        let mut n_minus_one = n_bytes();
        n_minus_one[31] -= 1;
        let mut one = [0_u8; 32];
        one[31] = 1;
        let top = Scalar::from_canonical_bytes(&n_minus_one).unwrap();
        let unit = Scalar::from_canonical_bytes(&one).unwrap();
        assert!(top.add(&unit).is_zero());
        let mut seven = [0_u8; 32];
        seven[31] = 7;
        let seven = Scalar::from_nonzero_canonical_bytes(&seven).unwrap();
        let mut product = [0_u8; 32];
        seven.multiply(&seven.invert()).write_bytes(&mut product);
        assert_eq!(product, one);
        assert!(Scalar::reduce_bytes(&n_bytes()).unwrap().is_zero());
    }
}
