//! Encryption of the login secret.

// --- Workspace dependencies ---
use crypto::{RandomSource, SecretVec};
use crypto_legacy::rsa::{RsaPublicKey, pkcs1v15::Pkcs1v15PublicOperations};

// --- Internal modules ---
use crate::{Error, hex};

/// The bytes RSAES-PKCS1-v1_5 adds to each block: two framing bytes, a separator, and at
/// least eight of padding.
const PADDING_OVERHEAD: usize = 11;

/// The longest modulus accepted, 8192 bits. The server names its key before it has been
/// authenticated, and the cost of using one grows steeply with its size.
const MAX_MODULUS_LEN: usize = 1024;

/// Encrypts a login secret (a password, or an answer to a challenge) for the gateway
/// (spec §3.4).
///
/// The plaintext is `secret`, an underscore, then the anti-replay value the server issued
/// most recently. It is cut into pieces that fit the key, each encrypted with RSAES-PKCS1-v1_5
/// under the gateway's public key, and the ciphertext blocks are concatenated as lowercase
/// hexadecimal. `pub_key` is the modulus in hexadecimal and `pub_key_exp` the exponent in
/// decimal, as `authConfig` gives them.
///
/// This padding has known weaknesses and is used because the protocol requires it.
///
/// # Errors
///
/// [`Error::Malformed`] when the key strings are not numbers, or the modulus is too small to
/// carry any plaintext or longer than 8192 bits; [`Error::Crypto`] when the key is not usable
/// or `random` fails.
pub fn encrypt_secret(
    pub_key: &str,
    pub_key_exp: &str,
    secret: &str,
    anti_replay_rand: &str,
    random: &mut impl RandomSource,
) -> Result<String, Error> {
    let modulus = hex::decode_number(pub_key)
        .filter(|modulus| modulus.len() <= MAX_MODULUS_LEN)
        .ok_or(Error::Malformed("public key modulus"))?;
    let exponent = pub_key_exp
        .parse::<u64>()
        .ok()
        .filter(|&exponent| exponent > 0)
        .ok_or(Error::Malformed("public key exponent"))?;
    let key = RsaPublicKey::from_components(&modulus, exponent.to_be_bytes())?;
    let chunk_len = key
        .modulus_len()
        .checked_sub(PADDING_OVERHEAD)
        .filter(|&len| len > 0)
        .ok_or(Error::Malformed("public key modulus"))?;

    // Held as a secret so that the copy of the password made here is zeroed when dropped.
    let plaintext = SecretVec::new([secret, "_", anti_replay_rand].concat().into_bytes());
    let mut output = String::new();
    for chunk in plaintext.expose_secret().chunks(chunk_len) {
        let block = key.encrypt_pkcs1v15(random, chunk)?;
        output.push_str(&hex::encode(block.as_bytes(), false));
    }
    Ok(output)
}
