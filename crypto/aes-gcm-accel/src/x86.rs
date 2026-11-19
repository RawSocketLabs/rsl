//! Checked x86-64 operations.

use core::arch::x86_64::{
    __cpuid, __m128i, _mm_aesenc_si128, _mm_aesenclast_si128, _mm_clmulepi64_si128,
    _mm_loadu_si128, _mm_storeu_si128, _mm_xor_si128,
};

/// Evidence that this host supports both GCM instructions. Only detection constructs it.
#[derive(Clone, Copy)]
pub struct AesGcmCpu(());

impl AesGcmCpu {
    /// CPUID.01H:ECX bits 25 and 1 advertise AES-NI and PCLMULQDQ respectively.
    #[must_use]
    pub fn detect() -> Option<Self> {
        const FEATURES: u32 = (1 << 25) | (1 << 1);
        // SAFETY: CPUID leaf 1 is available on x86-64 outside SGX.
        #[allow(unused_unsafe, reason = "CPUID requires unsafe on the workspace MSRV")]
        let flags = unsafe { __cpuid(1) }.ecx;
        (flags & FEATURES == FEATURES).then_some(Self(()))
    }

    /// Run FIPS 197 CIPHER with the eleven expanded AES-128 round keys.
    pub fn encrypt128(self, words: &[[u8; 4]; 44], block: &mut [u8; 16]) {
        // SAFETY: this token proves AES-NI is available.
        unsafe { encrypt(words, block) }
    }

    /// Run FIPS 197 CIPHER with the fifteen expanded AES-256 round keys.
    pub fn encrypt256(self, words: &[[u8; 4]; 60], block: &mut [u8; 16]) {
        // SAFETY: this token proves AES-NI is available.
        unsafe { encrypt(words, block) }
    }

    /// Multiply two GHASH blocks in NIST SP 800-38D section 6.3 displayed-bit order.
    #[must_use]
    pub fn multiply(self, left: &[u8; 16], right: &[u8; 16]) -> [u8; 16] {
        // SAFETY: this token proves PCLMULQDQ is available.
        unsafe { multiply(left, right) }
    }
}

#[target_feature(enable = "aes")]
unsafe fn encrypt(words: &[[u8; 4]], block: &mut [u8; 16]) {
    let mut keys = words.chunks_exact(4);
    // SAFETY: callers supply eleven or fifteen complete keys. Each group of four
    // contiguous words and the block cover all 16 bytes of an unaligned load/store.
    unsafe {
        let first = keys.next().expect("AES schedule includes an initial key");
        let last = keys.next_back().expect("AES schedule includes a final key");
        let mut state = _mm_xor_si128(
            _mm_loadu_si128(block.as_ptr().cast()),
            _mm_loadu_si128(first.as_ptr().cast()),
        );
        for key in keys {
            state = _mm_aesenc_si128(state, _mm_loadu_si128(key.as_ptr().cast()));
        }
        state = _mm_aesenclast_si128(state, _mm_loadu_si128(last.as_ptr().cast()));
        _mm_storeu_si128(block.as_mut_ptr().cast(), state);
    }
}

/// Reflect the displayed bits so register bit i represents coefficient x^i. Four
/// 64x64 carry-less products give the unreduced low and high halves.
#[target_feature(enable = "pclmulqdq")]
unsafe fn multiply(left: &[u8; 16], right: &[u8; 16]) -> [u8; 16] {
    // SAFETY: u128 and __m128i have identical size, and all bit patterns are valid.
    unsafe {
        let a = core::mem::transmute::<u128, __m128i>(u128::from_be_bytes(*left).reverse_bits());
        let b = core::mem::transmute::<u128, __m128i>(u128::from_be_bytes(*right).reverse_bits());
        let low = core::mem::transmute::<__m128i, u128>(_mm_clmulepi64_si128::<0x00>(a, b));
        let high = core::mem::transmute::<__m128i, u128>(_mm_clmulepi64_si128::<0x11>(a, b));
        let cross = core::mem::transmute::<__m128i, u128>(_mm_xor_si128(
            _mm_clmulepi64_si128::<0x01>(a, b),
            _mm_clmulepi64_si128::<0x10>(a, b),
        ));
        let low = low ^ (cross << 64);
        let high = high ^ (cross >> 64);
        // x^128 = x^7 + x^2 + x + 1. Fold the high half and then its seven overflow bits.
        let overflow = (high >> 121) ^ (high >> 126) ^ (high >> 127);
        let fold = |h: u128| h ^ (h << 1) ^ (h << 2) ^ (h << 7);
        (low ^ fold(high) ^ fold(overflow))
            .reverse_bits()
            .to_be_bytes()
    }
}
