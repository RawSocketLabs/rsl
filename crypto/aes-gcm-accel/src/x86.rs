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

    /// XOR eight-counter AES-128 groups, advancing inc32 and returning the byte count.
    #[must_use = "transform the remaining tail starting at the returned byte offset"]
    pub fn xor_groups128(
        self,
        words: &[[u8; 4]; 44],
        counter: &mut [u8; 16],
        data: &mut [u8],
    ) -> usize {
        // SAFETY: this token proves AES-NI is available.
        unsafe { xor_groups(words, counter, data) }
    }

    /// XOR eight-counter AES-256 groups, advancing inc32 and returning the byte count.
    #[must_use = "transform the remaining tail starting at the returned byte offset"]
    pub fn xor_groups256(
        self,
        words: &[[u8; 4]; 60],
        counter: &mut [u8; 16],
        data: &mut [u8],
    ) -> usize {
        // SAFETY: this token proves AES-NI is available.
        unsafe { xor_groups(words, counter, data) }
    }

    /// Multiply two GHASH blocks in NIST SP 800-38D section 6.3 displayed-bit order.
    #[must_use]
    pub fn multiply(self, left: &[u8; 16], right: &[u8; 16]) -> [u8; 16] {
        // SAFETY: this token proves PCLMULQDQ is available.
        unsafe {
            product(reflected(left), reflected(right))
                .reverse_bits()
                .to_be_bytes()
        }
    }

    /// Apply the GHASH recurrence to complete blocks in wire order.
    ///
    /// # Panics
    /// Panics if the byte length is not a multiple of sixteen.
    #[must_use]
    pub fn update_blocks(self, state: &[u8; 16], key: &[u8; 16], blocks: &[u8]) -> [u8; 16] {
        assert_eq!(blocks.len() % 16, 0);
        // SAFETY: this token proves PCLMULQDQ is available.
        unsafe { update_blocks(state, key, blocks) }
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

#[target_feature(enable = "aes")]
unsafe fn xor_groups(words: &[[u8; 4]], counter: &mut [u8; 16], data: &mut [u8]) -> usize {
    let prefix = u128::from_be_bytes(*counter) & !u128::from(u32::MAX);
    let mut next = u32::from_be_bytes(counter[12..].try_into().expect("four counter bytes"));
    let mut groups = data.chunks_exact_mut(8 * 16);
    let processed = groups.len() * 8 * 16;
    // SAFETY: each schedule chunk spans sixteen bytes, as does each lane in a complete group.
    unsafe {
        let mut keys = words.chunks_exact(4);
        let first = _mm_loadu_si128(keys.next().expect("initial key").as_ptr().cast());
        let last = _mm_loadu_si128(keys.next_back().expect("final key").as_ptr().cast());
        for group in &mut groups {
            let mut lanes: [__m128i; 8] = core::array::from_fn(|_| {
                let bytes = (prefix | u128::from(next)).to_be_bytes();
                next = next.wrapping_add(1);
                _mm_xor_si128(_mm_loadu_si128(bytes.as_ptr().cast()), first)
            });
            for key in keys.clone() {
                let key = _mm_loadu_si128(key.as_ptr().cast());
                for lane in &mut lanes {
                    *lane = _mm_aesenc_si128(*lane, key);
                }
            }
            for (lane, block) in lanes.into_iter().zip(group.chunks_exact_mut(16)) {
                let input = _mm_loadu_si128(block.as_ptr().cast());
                let output = _mm_xor_si128(input, _mm_aesenclast_si128(lane, last));
                _mm_storeu_si128(block.as_mut_ptr().cast(), output);
            }
        }
    }
    counter[12..].copy_from_slice(&next.to_be_bytes());
    processed
}

fn reflected(block: &[u8; 16]) -> u128 {
    u128::from_be_bytes(*block).reverse_bits()
}

#[target_feature(enable = "pclmulqdq")]
unsafe fn update_blocks(state: &[u8; 16], key: &[u8; 16], blocks: &[u8]) -> [u8; 16] {
    let mut acc = reflected(state);
    let h = reflected(key);
    for block in blocks.chunks_exact(16) {
        let block = block.try_into().expect("complete GHASH block");
        // SAFETY: this function has the same target feature as product.
        acc = unsafe { product(acc ^ reflected(block), h) };
    }
    acc.reverse_bits().to_be_bytes()
}

/// Multiply reflected field elements, where register bit i is the coefficient of x^i.
#[target_feature(enable = "pclmulqdq")]
unsafe fn product(left: u128, right: u128) -> u128 {
    // SAFETY: u128 and __m128i have identical size, and all bit patterns are valid.
    unsafe {
        let a = core::mem::transmute::<u128, __m128i>(left);
        let b = core::mem::transmute::<u128, __m128i>(right);
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
        low ^ fold(high) ^ fold(overflow)
    }
}
