//! The nonce generator.

// --- Internal modules ---
use crate::hex;

/// The generator of the 32-byte nonces a client sends, as 64 uppercase hexadecimal digits
/// (spec §3.3).
///
/// It is the Park–Miller "minimal standard" recurrence, seeded from the clock. It is **not**
/// a source of secrets: its whole state is the time it was created. The server only echoes a
/// nonce back, so nothing depends on it being unpredictable, and it is reproduced here only
/// so that a client looks like the vendor's.
#[derive(Clone, Debug)]
pub struct NonceGenerator {
    state: u64,
}

impl NonceGenerator {
    const MULTIPLIER: u64 = 48_271;
    const MODULUS: u64 = 2_147_483_647;
    /// A state at or above this (less one) yields no byte, so that every byte value is
    /// equally likely.
    const DISCARD_FROM: u64 = 0x7fff_ff00;

    /// A generator seeded with the Unix time in seconds.
    #[must_use]
    pub const fn new(unix_seconds: u64) -> Self {
        Self::from_state(unix_seconds)
    }

    /// A generator whose state is `state` reduced modulo 2³¹−1, or 1 when that is 0, a value
    /// the recurrence could never leave. The reduction keeps the state where the recurrence
    /// keeps it, so the arithmetic below cannot overflow.
    #[must_use]
    pub const fn from_state(state: u64) -> Self {
        let state = state % Self::MODULUS;
        Self {
            state: if state == 0 { 1 } else { state },
        }
    }

    /// The next nonce: 32 bytes as 64 uppercase hexadecimal digits.
    pub fn next_nonce(&mut self) -> String {
        let mut bytes = [0_u8; 32];
        for byte in &mut bytes {
            *byte = loop {
                self.state = self.state * Self::MULTIPLIER % Self::MODULUS;
                let value = self.state - 1;
                if value < Self::DISCARD_FROM {
                    break value.to_le_bytes()[0];
                }
            };
        }
        hex::encode(&bytes, true)
    }
}
