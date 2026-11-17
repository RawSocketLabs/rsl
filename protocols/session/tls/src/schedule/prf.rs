//! The TLS 1.2 pseudorandom function.

// --- Workspace dependencies ---
use crypto::{CryptoError, SecretBytes, mac::hmac::sha256::HmacSha256};

/// The output length of HMAC-SHA-256, and so of one PRF block.
const BLOCK_LEN: usize = 32;

/// Fills `output` with `PRF(secret, label, seed)`, the TLS 1.2 PRF on HMAC-SHA-256
/// (RFC 5246 §5). Every TLS 1.2 secret and check value comes from this function; it is public
/// for callers that derive their own values from a connection's secrets.
///
/// # Errors
///
/// Only when an input is too long for SHA-256 to hash.
//~ models rfc5246#5
pub fn prf_sha256(
    secret: &[u8],
    label: &[u8],
    seed: &[u8],
    output: &mut [u8],
) -> Result<(), CryptoError> {
    // A(1) = HMAC(secret, label + seed); each block is HMAC(secret, A(i) + label + seed).
    // Both are as secret as the output: a block of "key expansion" is the record keys.
    let mut a = hmac(secret, &[label, seed])?;
    for block in output.chunks_mut(BLOCK_LEN) {
        let bytes = hmac(secret, &[a.expose_secret(), label, seed])?;
        block.copy_from_slice(&bytes.expose_secret()[..block.len()]);
        a = hmac(secret, &[a.expose_secret()])?;
    }
    Ok(())
}

/// HMAC-SHA-256 of `parts` concatenated, held as a secret so it is zeroed when dropped.
fn hmac(secret: &[u8], parts: &[&[u8]]) -> Result<SecretBytes<BLOCK_LEN>, CryptoError> {
    let mut mac = HmacSha256::new(secret)?;
    for part in parts {
        mac.update(part)?;
    }
    Ok(SecretBytes::new(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod unit {
    use super::*;

    /// `openssl kdf -keylen 100 -kdfopt digest:SHA256 -kdfopt hexsecret:000102…0f
    /// -kdfopt hexseed:<"test label" then f0f1…ff> TLS1-PRF` (OpenSSL 4.0.3, 2026-11-16).
    /// 100 bytes takes four blocks and a partial last one.
    #[test]
    fn output_matches_openssl_across_several_blocks() {
        let secret: Vec<u8> = (0x00..=0x0f).collect();
        let seed: Vec<u8> = (0xf0..=0xff).collect();
        let mut output = [0; 100];
        prf_sha256(&secret, b"test label", &seed, &mut output).unwrap();
        let expected = concat!(
            "d40ba15a6bc5c779480929343115c3e732e19d9316fc08ca0dd9829443111cd2",
            "e21ac6756ab4729a89d5a089926cfa2e3cb32a3f796f7b72ad4090d6127121f2",
            "a06f88b9554ed94ca320697ffabbdcbffc75e258889b4f5ff43275c0bae9db82",
            "73480707",
        );
        let expected: Vec<u8> = (0..expected.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&expected[at..at + 2], 16).unwrap())
            .collect();
        assert_eq!(output.to_vec(), expected);
    }
}
