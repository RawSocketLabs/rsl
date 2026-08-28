//! NIST P-384 (secp384r1): the parameter set and its evidence.
//!
//! # What P-384 is
//!
//! P-384 is the short Weierstrass curve `y^2 = x^3 - 3x + b` over `GF(p)`,
//! `p = 2^384 - 2^128 - 2^96 + 2^32 - 1`, with a prime group order `n` and cofactor one. Its
//! domain parameters are published in NIST SP 800-186 §3.2.1.4. TLS 1.3 names the curve
//! `secp384r1` for key exchange and `ecdsa_secp384r1_sha384` for signatures; SSH names it
//! `ecdh-sha2-nistp384` and `ecdsa-sha2-nistp384`.
//!
//! The arithmetic lives in [`crate::curve::weierstrass`], generic over the [`Curve`] parameter
//! trait; P-384 needs six 64-bit limbs and only two reduction folds because `2^384 - p` has just
//! 129 bits. This module supplies the constants as [`P384`] and the type aliases the ECDH and
//! ECDSA schemes use.
//!
//! # Evidence and security status
//!
//! The private tests check the published hexadecimal forms of `p`, `n`, `b`, and `G`, the
//! generator's curve membership, `[n]G = O`, and the RFC 5903 §8.2 public points derived from
//! their published private keys. Public evidence lives with the ECDH and ECDSA consumers. No
//! side-channel or audit claim is made.

#![allow(rustdoc::private_intra_doc_links)]

use crate::curve::weierstrass::{self, Curve, arithmetic::Modulus};

/// Limbs in a P-384 element.
pub(crate) const LIMBS: usize = 6;
/// Bytes in a P-384 coordinate or scalar.
pub(crate) const ELEMENT_BYTES: usize = weierstrass::element_bytes::<LIMBS>();
/// Bytes in an uncompressed P-384 point.
pub(crate) const ENCODED_LEN: usize = weierstrass::encoded_point_bytes::<LIMBS>();

/// The SP 800-186 §3.2.1.4 parameter set.
#[derive(Clone, Copy)]
pub(crate) struct P384;

impl Curve<LIMBS> for P384 {
    /// `p = 2^384 - 2^128 - 2^96 + 2^32 - 1`.
    const FIELD: Modulus<LIMBS> = Modulus::new([
        0x0000_0000_ffff_ffff,
        0xffff_ffff_0000_0000,
        0xffff_ffff_ffff_fffe,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
    ]);
    const ORDER: Modulus<LIMBS> = Modulus::new([
        0xecec_196a_ccc5_2973,
        0x581a_0db2_48b0_a77a,
        0xc763_4d81_f437_2ddf,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
    ]);
    const FIELD_INVERSION_EXPONENT: [u64; LIMBS] = [
        0x0000_0000_ffff_fffd,
        0xffff_ffff_0000_0000,
        0xffff_ffff_ffff_fffe,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
    ];
    const ORDER_INVERSION_EXPONENT: [u64; LIMBS] = [
        0xecec_196a_ccc5_2971,
        0x581a_0db2_48b0_a77a,
        0xc763_4d81_f437_2ddf,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
    ];
    const B: [u64; LIMBS] = [
        0x2a85_c8ed_d3ec_2aef,
        0xc656_398d_8a2e_d19d,
        0x0314_088f_5013_875a,
        0x181d_9c6e_fe81_4112,
        0x988e_056b_e3f8_2d19,
        0xb331_2fa7_e23e_e7e4,
    ];
    const GENERATOR_X: [u64; LIMBS] = [
        0x3a54_5e38_7276_0ab7,
        0x5502_f25d_bf55_296c,
        0x59f7_41e0_8254_2a38,
        0x6e1d_3b62_8ba7_9b98,
        0x8eb1_c71e_f320_ad74,
        0xaa87_ca22_be8b_0537,
    ];
    const GENERATOR_Y: [u64; LIMBS] = [
        0x7a43_1d7c_90ea_0e5f,
        0x0a60_b1ce_1d7e_819d,
        0xe9da_3113_b5f0_b8c0,
        0xf8f4_1dbd_289a_147c,
        0x5d9e_98bf_9292_dc29,
        0x3617_de4a_9626_2c6f,
    ];
    const NAME: &'static str = "P-384 scalar";
}

pub(crate) type Scalar = weierstrass::scalar::Scalar<P384, LIMBS>;
pub(crate) type ProjectivePoint = weierstrass::point::ProjectivePoint<P384, LIMBS>;
pub(crate) type AffinePoint = weierstrass::point::AffinePoint<P384, LIMBS>;

/// FIPS 186-5 A.2.2 candidate testing for a 48-byte P-384 private scalar.
pub(crate) fn generate_private_bytes<R: crate::random::RandomSource>(
    random: &mut R,
) -> crate::Result<[u8; ELEMENT_BYTES]> {
    let mut out = [0_u8; ELEMENT_BYTES];
    weierstrass::scalar::generate_private_bytes::<P384, LIMBS, R>(random, &mut out)?;
    Ok(out)
}

/// Current project lifecycle classification for the P-384 group arithmetic.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;

#[cfg(test)]
mod unit {
    use super::*;
    use crate::curve::weierstrass::arithmetic;

    fn decode<const N: usize>(hex: &str) -> [u8; N] {
        core::array::from_fn(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
    }

    fn n_bytes() -> [u8; 48] {
        let mut out = [0_u8; 48];
        arithmetic::write_be_bytes(&P384::ORDER.value, &mut out);
        out
    }

    /// Published evidence: SP 800-186 §3.2.1.4 hexadecimal forms of `p`, `n`, `b`, and `G`.
    #[test]
    fn parameters_match_the_published_hexadecimal_forms() {
        let mut p = [0_u8; 48];
        arithmetic::write_be_bytes(&P384::FIELD.value, &mut p);
        assert_eq!(
            p,
            decode::<48>(
                "fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffeffffffff0000000000000000ffffffff"
            )
        );
        assert_eq!(
            n_bytes(),
            decode::<48>(
                "ffffffffffffffffffffffffffffffffffffffffffffffffc7634d81f4372ddf581a0db248b0a77aecec196accc52973"
            )
        );
        let mut b = [0_u8; 48];
        arithmetic::write_be_bytes(&P384::B, &mut b);
        assert_eq!(
            b,
            decode::<48>(
                "b3312fa7e23ee7e4988e056be3f82d19181d9c6efe8141120314088f5013875ac656398d8a2ed19d2a85c8edd3ec2aef"
            )
        );
        let mut g = [0_u8; ENCODED_LEN];
        ProjectivePoint::generator()
            .to_affine()
            .unwrap()
            .write_bytes(&mut g);
        assert_eq!(
            g[1..49],
            decode::<48>(
                "aa87ca22be8b05378eb1c71ef320ad746e1d3b628ba79b9859f741e082542a385502f25dbf55296c3a545e3872760ab7"
            )
        );
        assert_eq!(
            g[49..],
            decode::<48>(
                "3617de4a96262c6f5d9e98bf9292dc29f8f41dbd289a147ce9da3113b5f0b8c00a60b1ce1d7e819d7a431d7c90ea0e5f"
            )
        );
    }

    #[test]
    fn generator_is_on_the_curve_and_has_order_n() {
        let mut g = [0_u8; ENCODED_LEN];
        ProjectivePoint::generator()
            .to_affine()
            .unwrap()
            .write_bytes(&mut g);
        assert!(AffinePoint::from_bytes(&g).is_some());
        assert!(
            ProjectivePoint::generator()
                .multiply(&n_bytes())
                .is_identity()
        );
        let mut off_curve = g;
        off_curve[96] ^= 1;
        assert!(AffinePoint::from_bytes(&off_curve).is_none());
    }

    /// Published evidence: RFC 5903 §8.2 initiator public point from its private key.
    #[test]
    fn rfc_5903_initiator_public_point_derives_from_the_published_private_key() {
        let private = decode::<48>(
            "099f3c7034d4a2c699884d73a375a67f7624ef7c6b3c0f160647b67414dce655e35b538041e649ee3faef896783ab194",
        );
        let mut public = [0_u8; ENCODED_LEN];
        ProjectivePoint::generator()
            .multiply(&private)
            .to_affine()
            .unwrap()
            .write_bytes(&mut public);
        assert_eq!(
            public[1..49],
            decode::<48>(
                "667842d7d180ac2cde6f74f37551f55755c7645c20ef73e31634fe72b4c55ee6de3ac808acb4bdb4c88732aee95f41aa"
            )
        );
        assert_eq!(
            public[49..],
            decode::<48>(
                "9482ed1fc0eeb9cafc4984625ccfc23f65032149e0e144ada024181535a0f38eeb9fcff3c2c947dae69b4c634573a81c"
            )
        );
    }
}
