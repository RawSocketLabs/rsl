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
    #[cfg(feature = "aes-gcm-accel")]
    fn xor_groups(&self, _: &mut [u8; 16], _: &mut [u8]) -> usize {
        0
    }
    #[cfg(feature = "aes-gcm-accel")]
    fn accelerator(&self) -> Option<rsl_aes_gcm_accel::AesGcmCpu> {
        None
    }
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

/// Key schedule plus the capability selected once for this GCM key.
#[cfg(feature = "aes-gcm-accel")]
pub(super) struct Selected<C> {
    cipher: C,
    cpu: Option<rsl_aes_gcm_accel::AesGcmCpu>,
}

#[cfg(feature = "aes-gcm-accel")]
impl<C> From<C> for Selected<C> {
    fn from(cipher: C) -> Self {
        Self {
            cipher,
            cpu: rsl_aes_gcm_accel::AesGcmCpu::detect(),
        }
    }
}

#[cfg(not(feature = "aes-gcm-accel"))]
pub(super) type Selected<C> = C;

#[cfg(feature = "aes-gcm-accel")]
impl GcmBlockCipher for Selected<Aes128> {
    fn encrypt_block_in_place(&self, block: &mut [u8; 16]) {
        if let Some(cpu) = self.cpu {
            cpu.encrypt128(self.cipher.expanded_words(), block);
        } else {
            self.cipher.encrypt_block_in_place(block);
        }
    }
    fn xor_groups(&self, counter: &mut [u8; 16], data: &mut [u8]) -> usize {
        self.cpu.map_or(0, |cpu| {
            cpu.xor_groups128(self.cipher.expanded_words(), counter, data)
        })
    }
    fn accelerator(&self) -> Option<rsl_aes_gcm_accel::AesGcmCpu> {
        self.cpu
    }
}

#[cfg(feature = "aes-gcm-accel")]
impl GcmBlockCipher for Selected<Aes256> {
    fn encrypt_block_in_place(&self, block: &mut [u8; 16]) {
        if let Some(cpu) = self.cpu {
            cpu.encrypt256(self.cipher.expanded_words(), block);
        } else {
            self.cipher.encrypt_block_in_place(block);
        }
    }
    fn xor_groups(&self, counter: &mut [u8; 16], data: &mut [u8]) -> usize {
        self.cpu.map_or(0, |cpu| {
            cpu.xor_groups256(self.cipher.expanded_words(), counter, data)
        })
    }
    fn accelerator(&self) -> Option<rsl_aes_gcm_accel::AesGcmCpu> {
        self.cpu
    }
}

#[cfg(all(
    test,
    feature = "aes-gcm-accel",
    target_arch = "x86_64",
    not(target_env = "sgx"),
    not(target_os = "none")
))]
mod tests {
    extern crate std;

    use super::*;
    use crate::cipher::aes::aes128::Aes128Key;

    #[test]
    fn selection_matches_the_host_and_fallback_encrypts_the_same_block() {
        let aes = Aes128::new(Aes128Key::new([0x55; 16]));
        let selected = Selected::from(aes);
        assert_eq!(
            selected.accelerator().is_some(),
            std::is_x86_feature_detected!("aes") && std::is_x86_feature_detected!("pclmulqdq")
        );
        let mut hardware = [0x13; 16];
        let mut portable = hardware;
        selected.encrypt_block_in_place(&mut hardware);
        let fallback = Selected {
            cipher: selected.cipher,
            cpu: None,
        };
        fallback.encrypt_block_in_place(&mut portable);
        assert_eq!(hardware, portable);
    }
}
