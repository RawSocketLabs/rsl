//! The one block-cipher operation GCM needs, abstracted over the AES key size.
//!
//! SP 800-38D §5.1 defines GCM over any approved 128-bit block cipher and uses only the forward
//! direction `CIPH_K`. This crate-private trait lets the GCTR, hash-subkey, tag, seal, and open
//! layers be written once for AES-128 and AES-256 without exposing a public generic API.

use crate::cipher::aes::{
    aes128::{Aes128, Aes128Block},
    aes256::Aes256,
};

/// `CIPH_K` over one 16-byte block, in place.
pub(super) trait GcmBlockCipher {
    fn encrypt_block_in_place(&self, block: &mut [u8; 16]);
}

impl GcmBlockCipher for Aes128 {
    fn encrypt_block_in_place(&self, block: &mut [u8; 16]) {
        let mut owned = Aes128Block::new(*block);
        self.encrypt_block(&mut owned);
        *block = owned.into_bytes();
    }
}

impl GcmBlockCipher for Aes256 {
    fn encrypt_block_in_place(&self, block: &mut [u8; 16]) {
        let mut owned = Aes128Block::new(*block);
        self.encrypt_block(&mut owned);
        *block = owned.into_bytes();
    }
}
