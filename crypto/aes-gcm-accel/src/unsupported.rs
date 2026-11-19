//! Targets without a usable x86-64 backend cannot construct the capability.

/// Uninhabited on unsupported targets, so callers always use their portable path.
#[derive(Clone, Copy)]
pub enum AesGcmCpu {}

impl AesGcmCpu {
    /// This target has no accelerated implementation.
    #[must_use]
    pub const fn detect() -> Option<Self> {
        None
    }

    /// AES-128 round function, unreachable without a capability.
    pub fn encrypt128(self, _: &[[u8; 4]; 44], _: &mut [u8; 16]) {
        match self {}
    }

    /// AES-256 round function, unreachable without a capability.
    pub fn encrypt256(self, _: &[[u8; 4]; 60], _: &mut [u8; 16]) {
        match self {}
    }

    /// GHASH field product, unreachable without a capability.
    #[must_use]
    pub fn multiply(self, _: &[u8; 16], _: &[u8; 16]) -> [u8; 16] {
        match self {}
    }
}
