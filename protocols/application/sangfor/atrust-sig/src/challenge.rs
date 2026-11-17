//! The challenge check of the anti-MITM handshake.

// --- Workspace dependencies ---
use crypto::{
    SecretVec,
    cipher::aes::aes128::{Aes128, Aes128Block, Aes128Key},
    digest::sha2::sha256::Sha256,
};
use crypto_legacy::cipher::cbc::{CbcState, encrypt_blocks};

// --- Internal modules ---
use crate::{Error, hex};

/// Mixed into the challenge check's key material. Embedded in the vendor client
/// (spec §3.1, `C_chal`).
const CHALLENGE_CONSTANT: &[u8] =
    b"OrHWuJz7gku5awmVb5w1sKTmfeCWHmzokBxmn0sn0faIcv1G10PdrbbRGKBrrZ3m";

/// Whether the server's `encryptedChallenge` is the encryption of its `challenge` under the
/// key and IV its device key implies (spec §3.3).
///
/// The key and IV are the two halves of a SHA-256 over the device key strings and a
/// constant; the challenge is padded as in PKCS #7 and encrypted with AES-128-CBC; the
/// result, as uppercase hexadecimal, must equal `encrypted_challenge` exactly. The check
/// cannot pass when any of the four inputs is empty.
///
/// # Errors
///
/// Only if an input is too long for SHA-256 to hash.
pub fn challenge_passes(
    device_pub_key_mod: &str,
    device_pub_key_exp: &str,
    challenge: &str,
    encrypted_challenge: &str,
) -> Result<bool, Error> {
    let inputs = [
        device_pub_key_mod,
        device_pub_key_exp,
        challenge,
        encrypted_challenge,
    ];
    if inputs.iter().any(|input| input.is_empty()) {
        return Ok(false);
    }
    let mut material = Sha256::new();
    material.update(device_pub_key_mod)?;
    material.update(device_pub_key_exp)?;
    material.update(CHALLENGE_CONSTANT)?;
    let material = material.finalize().into_bytes();
    let (key, iv) = material.split_at(16);
    let key = Aes128Key::new(
        key.try_into()
            .map_err(|_| Error::Malformed("key material"))?,
    );
    let iv = Aes128Block::new(
        iv.try_into()
            .map_err(|_| Error::Malformed("key material"))?,
    );

    // PKCS #7: one to sixteen bytes, each the count of bytes added.
    let mut padded = challenge.as_bytes().to_vec();
    let padding = 16 - padded.len() % 16;
    padded.resize(padded.len() + padding, u8::try_from(padding).unwrap_or(16));
    let mut blocks: Vec<Aes128Block> = padded
        .chunks_exact(16)
        .filter_map(|block| block.try_into().ok().map(Aes128Block::new))
        .collect();
    encrypt_blocks(&Aes128::new(key), &mut CbcState::new(iv), &mut blocks)?;

    let ciphertext: Vec<u8> = blocks.iter().flat_map(|block| *block.as_bytes()).collect();
    let expected = SecretVec::new(hex::encode(&ciphertext, true).into_bytes());
    Ok(expected.matches(encrypted_challenge))
}
