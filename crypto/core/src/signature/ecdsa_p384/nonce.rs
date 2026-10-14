//! RFC 6979 §3.2 deterministic per-signature secret `k` for P-384 with SHA-384.
//!
//! ## Standards ownership
//!
//! RFC 6979 §3.2 derives `k` from the private key `x` and the message digest `h1` using an
//! HMAC_DRBG-style construction keyed by the same hash. With SHA-384 and P-384, `hlen = qlen =
//! 384` bits, so §2.3.2 `bits2int` keeps the whole 48-byte string, §2.3.3 `int2octets` is the
//! 48-byte big-endian scalar encoding, and §2.3.4 `bits2octets(h1)` is `int2octets(h1 mod n)`.
//! Every step is written out with the RFC's letters so it can be checked line by line.
//!
//! §3.2 step h.3 compares the candidate with `n` instead of reducing it; a rejected candidate
//! or a candidate that yields `r = 0` or `s = 0` triggers the RFC's retry update.

use zeroize::Zeroize;

use crate::{Result, curve::p384::Scalar, mac::hmac::sha384::HmacSha384};

/// `8 * ceil(hlen / 8)` bits of SHA-384 output, in bytes.
const HLEN_BYTES: usize = 48;

/// RFC 6979 §3.2 state `(K, V)`; zeroized on drop because it determines `k`.
pub(super) struct NonceGenerator {
    k: [u8; HLEN_BYTES],
    v: [u8; HLEN_BYTES],
}

impl NonceGenerator {
    /// Steps a–g: seed `(K, V)` from `int2octets(x)` and `bits2octets(h1)`.
    ///
    /// # Errors
    ///
    /// Propagates the HMAC length error, which the fixed 129-byte inputs cannot trigger.
    pub(super) fn new(private_scalar: &[u8; 48], digest: &[u8; 48]) -> Result<Self> {
        // §2.3.4: bits2octets(h1) = int2octets(bits2int(h1) mod q).
        let mut reduced_digest = [0_u8; 48];
        Scalar::reduce_bytes(digest)
            .expect("a SHA-384 digest is exactly 32 bytes")
            .write_bytes(&mut reduced_digest);

        // b, c.
        let mut generator = Self {
            v: [0x01; HLEN_BYTES],
            k: [0x00; HLEN_BYTES],
        };
        // d: K = HMAC_K(V || 0x00 || int2octets(x) || bits2octets(h1)).
        generator.k = generator.hmac(&[&generator.v, &[0x00], private_scalar, &reduced_digest])?;
        // e: V = HMAC_K(V).
        generator.v = generator.hmac(&[&generator.v])?;
        // f: K = HMAC_K(V || 0x01 || int2octets(x) || bits2octets(h1)).
        generator.k = generator.hmac(&[&generator.v, &[0x01], private_scalar, &reduced_digest])?;
        // g: V = HMAC_K(V).
        generator.v = generator.hmac(&[&generator.v])?;

        reduced_digest.zeroize();
        Ok(generator)
    }

    /// Steps h.1–h.2: produce `T`, which is exactly one `V` block when `qlen = hlen`.
    ///
    /// The caller applies step h.3's range and `r`/`s` checks and, on rejection, calls
    /// [`Self::reject`].
    pub(super) fn candidate(&mut self) -> Result<[u8; 48]> {
        self.v = self.hmac(&[&self.v])?;
        Ok(self.v)
    }

    /// Step h.3 retry update: `K = HMAC_K(V || 0x00)`, `V = HMAC_K(V)`.
    pub(super) fn reject(&mut self) -> Result<()> {
        self.k = self.hmac(&[&self.v, &[0x00]])?;
        self.v = self.hmac(&[&self.v])?;
        Ok(())
    }

    fn hmac(&self, parts: &[&[u8]]) -> Result<[u8; HLEN_BYTES]> {
        let mut mac = HmacSha384::new(&self.k)?;
        for part in parts {
            mac.update(part)?;
        }
        Ok(mac.finalize().into_bytes())
    }
}

impl Drop for NonceGenerator {
    fn drop(&mut self) {
        self.k.zeroize();
        self.v.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::digest::sha2::sha384::Sha384;

    fn decode(hex: &str) -> [u8; 48] {
        core::array::from_fn(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
    }

    /// RFC 6979 A.2.6 private key `x`.
    const X: &str = "6b9d3dad2e1b8c1c05b19875b6659f4de23c3b667bf297ba9aa47740787137d896d5724e4c70a825f872c9ea60d2edf5";

    /// Published evidence: A.2.6's SHA-384 `k` values for "sample" and "test" are reproduced.
    #[test]
    fn rfc_6979_published_k_values_are_reproduced() {
        let cases = [
            (
                &b"sample"[..],
                "94ed910d1a099dad3254e9242ae85abde4ba15168eaf0ca87a555fd56d10fbca2907e3e83ba95368623b8c4686915cf9",
            ),
            (
                &b"test"[..],
                "015ee46a5bf88773ed9123a5ab0807962d193719503c527b031b4c2d225092ada71f4a459bc0da98adb95837db8312ea",
            ),
        ];
        for (message, expected_k) in cases {
            let digest = Sha384::digest(message).unwrap().into_bytes();
            let mut generator = NonceGenerator::new(&decode(X), &digest).unwrap();
            assert_eq!(generator.candidate().unwrap(), decode(expected_k));
        }
    }

    /// Standard-derived evidence: the retry update yields a different candidate.
    #[test]
    fn rejection_advances_the_generator_state() {
        let digest = Sha384::digest(b"sample").unwrap().into_bytes();
        let mut generator = NonceGenerator::new(&decode(X), &digest).unwrap();
        let first = generator.candidate().unwrap();
        generator.reject().unwrap();
        let second = generator.candidate().unwrap();
        assert_ne!(first, second);
    }
}
