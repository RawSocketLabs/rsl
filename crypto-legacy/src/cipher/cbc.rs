//! Narrow, stateful Cipher Block Chaining (CBC) from NIST SP 800-38A §6.2.
//!
//! > **CBC supplies confidentiality mechanics only. It does not authenticate ciphertext.**
//!
//! For encryption, each plaintext block is combined with the current chaining value using XOR
//! before the block
//! cipher runs. The resulting ciphertext becomes the next chaining value. Decryption first saves
//! the ciphertext, applies the inverse block permutation, XORs the previous chaining value, and
//! then advances the chain to the saved ciphertext.
//!
//! # Exact scope
//!
//! [`encrypt_blocks`] and [`decrypt_blocks`] accept only complete, already-separated blocks. They
//! do not add or validate padding, generate an IV, prepend an IV, authenticate bytes, combine a
//! MAC, enforce a key-usage limit, release-gate plaintext, or frame records. Those choices differ
//! across SSL, TLS, SSH, storage formats, and other historical protocols and remain in the
//! repository that defines each profile.
//!
//! ```
//! use rsl_crypto_legacy::cipher::{
//!     cbc::{CbcState, decrypt_blocks, encrypt_blocks},
//!     des::{DesBlock, TripleDesEde3, TripleDesEde3Key},
//! };
//!
//! let key = TripleDesEde3Key::new([
//!     0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
//!     0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01,
//!     0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23,
//! ]);
//! let cipher = TripleDesEde3::new(key);
//! let iv = DesBlock::new([0xf6, 0x9f, 0x24, 0x45, 0xdf, 0x4f, 0x9b, 0x17]);
//! let plaintext = [0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96];
//! let mut blocks = [DesBlock::new(plaintext)];
//! let mut encryption_chain = CbcState::new(iv);
//!
//! encrypt_blocks(&cipher, &mut encryption_chain, &mut blocks)?;
//! assert_eq!(blocks[0].as_bytes(), &[0x20, 0x79, 0xc3, 0xd5, 0x3a, 0xa7, 0x63, 0xe1]);
//!
//! let mut decryption_chain = CbcState::new(DesBlock::new([
//!     0xf6, 0x9f, 0x24, 0x45, 0xdf, 0x4f, 0x9b, 0x17,
//! ]));
//! decrypt_blocks(&cipher, &mut decryption_chain, &mut blocks)?;
//! assert_eq!(blocks[0].as_bytes(), &plaintext);
//! # Ok::<(), rsl_crypto_legacy::CryptoError>(())
//! ```
//!
//! SP 800-38A remains the mathematical baseline while NIST is preparing a revision. RFC 9325
//! documents why a TLS CBC suite should not be used unless Encrypt-then-MAC was negotiated. The
//! standards ledger records exact sources, mapping, evidence, and exclusions.

use alloc::vec::Vec;
use core::fmt;

use crate::{CryptoError, Result};
use rsl_crypto::cipher::BlockCipher;

/// One CBC direction's current chaining block.
///
/// At construction this contains the profile-supplied IV. After processing a block it contains
/// that block's ciphertext. Encryption and decryption directions need independent states.
pub struct CbcState<B> {
    block: B,
}

impl<B> CbcState<B> {
    /// Begin one CBC direction with a typed block containing the IV.
    #[must_use]
    pub const fn new(initial_value: B) -> Self {
        Self {
            block: initial_value,
        }
    }

    /// Consume the state and return the current chaining block.
    #[must_use]
    pub fn into_block(self) -> B {
        self.block
    }
}

impl<B: AsRef<[u8]>> CbcState<B> {
    /// Borrow the current IV/ciphertext chaining bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.block.as_ref()
    }
}

impl<B: AsRef<[u8]>> fmt::Debug for CbcState<B> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CbcState")
            .field("block_length", &self.block.as_ref().len())
            .field("chaining_bytes", &"[REDACTED]")
            .finish()
    }
}

/// Encrypt complete blocks in place and advance `state` to the final ciphertext block.
///
/// Calling this repeatedly with the same state is equivalent to one call over the concatenated
/// block sequence.
///
/// # Errors
///
/// Returns [`CryptoError::InvalidLength`] before mutation if a supplied block's runtime byte
/// length differs from the chaining block's length. Ordinary fixed-size block types make this
/// error unreachable, but the generic contract validates custom implementations defensively.
pub fn encrypt_blocks<C>(
    cipher: &C,
    state: &mut CbcState<C::Block>,
    blocks: &mut [C::Block],
) -> Result<()>
where
    C: BlockCipher,
{
    validate_lengths(state, blocks)?;
    for block in blocks {
        xor_in_place(block.as_mut(), state.block.as_ref());
        cipher.encrypt_block(block);
        state.block.as_mut().copy_from_slice(block.as_ref());
    }
    Ok(())
}

/// Decrypt complete blocks in place and advance `state` to the final input ciphertext block.
///
/// The temporary allocation stores ciphertext needed for the next chain step. It is intentionally
/// obvious and local: accuracy and generic readability take priority over avoiding an allocation
/// in this reference path.
///
/// # Errors
///
/// Returns [`CryptoError::InvalidLength`] before mutation if a supplied block's runtime byte
/// length differs from the chaining block's length.
pub fn decrypt_blocks<C>(
    cipher: &C,
    state: &mut CbcState<C::Block>,
    blocks: &mut [C::Block],
) -> Result<()>
where
    C: BlockCipher,
{
    validate_lengths(state, blocks)?;
    let mut saved_ciphertext = Vec::with_capacity(state.block.as_ref().len());
    for block in blocks {
        saved_ciphertext.clear();
        saved_ciphertext.extend_from_slice(block.as_ref());
        cipher.decrypt_block(block);
        xor_in_place(block.as_mut(), state.block.as_ref());
        state.block.as_mut().copy_from_slice(&saved_ciphertext);
    }
    Ok(())
}

fn validate_lengths<B: AsRef<[u8]>>(state: &CbcState<B>, blocks: &[B]) -> Result<()> {
    let expected = state.block.as_ref().len();
    for block in blocks {
        let actual = block.as_ref().len();
        if actual != expected {
            return Err(CryptoError::InvalidLength {
                name: "CBC block",
                expected,
                actual,
            });
        }
    }
    Ok(())
}

fn xor_in_place(output: &mut [u8], input: &[u8]) {
    debug_assert_eq!(output.len(), input.len());
    for (output_byte, input_byte) in output.iter_mut().zip(input) {
        *output_byte ^= input_byte;
    }
}

/// CBC's package lifecycle status: isolated for explicit historical compatibility profiles.
pub const SECURITY_STATUS: crate::SecurityStatus = crate::SecurityStatus::Legacy;
