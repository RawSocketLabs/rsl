//! **os-random** — the operating system's random generator as a [`crypto::RandomSource`].
//!
//! `crypto` defines the [`RandomSource`] boundary and deliberately names no platform
//! generator. This crate is that choice, made in one reviewed place: [`OsRandom`] fills from
//! the operating system's cryptographic generator through the `getrandom` crate.
//!
//! Protocol crates keep taking `&mut impl RandomSource`, so tests can pass a reproducible
//! source and a deployment can pass a hardware one. Only the code that owns a program's
//! policy (a binary, an example, a convenience constructor) names [`OsRandom`].
//!
//! ```
//! use crypto::RandomSource;
//! use os_random::OsRandom;
//!
//! let mut key = [0_u8; 32];
//! OsRandom.fill_bytes(&mut key)?;
//! # Ok::<(), crypto::CryptoError>(())
//! ```

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use crypto::{CryptoError, RandomSource, Result};

/// The operating system's cryptographic random generator.
///
/// - **It waits rather than degrades.** Early in boot, before the kernel's generator has been
///   seeded, a call blocks until it is. That is the right behaviour for key generation, and
///   it means a unit started very early can pause here without logging anything.
/// - **It is stateless.** Nothing is buffered between calls, so there is no state to
///   duplicate across a fork or a snapshot, and every value of this type is the same source.
/// - **On error the buffer's contents are unspecified.** Do not use them.
#[derive(Debug)]
pub struct OsRandom;

impl RandomSource for OsRandom {
    /// Fills all of `output` from the operating system, or fails without claiming any of it.
    ///
    /// # Errors
    ///
    /// [`CryptoError::EntropyUnavailable`] when the operating system cannot supply the bytes.
    fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
        getrandom::fill(output).map_err(|_| CryptoError::EntropyUnavailable)
    }
}

#[cfg(test)]
mod unit {
    extern crate alloc;

    use alloc::vec;

    use super::*;

    #[test]
    fn every_length_is_filled_without_error() {
        for length in [0, 1, 64, 256, 257, 4096] {
            let mut output = vec![0_u8; length];
            assert_eq!(OsRandom.fill_bytes(&mut output), Ok(()), "{length} bytes");
        }
    }

    /// A fill that stopped short would leave the same bytes zero every time. Sixteen draws
    /// OR-ed together leave a correctly filled byte zero with probability 2^-128, so across
    /// 4096 bytes a false failure has probability about 2^-116.
    #[test]
    fn every_position_of_a_long_buffer_is_written() {
        let mut seen = [0_u8; 4096];
        for _ in 0..16 {
            let mut output = [0_u8; 4096];
            OsRandom.fill_bytes(&mut output).unwrap();
            for (seen, byte) in seen.iter_mut().zip(output) {
                *seen |= byte;
            }
        }
        let unwritten = seen.iter().position(|&byte| byte == 0);
        assert_eq!(unwritten, None, "a position no draw ever wrote");
    }

    #[test]
    fn two_draws_differ() {
        let (mut first, mut second) = ([0_u8; 32], [0_u8; 32]);
        OsRandom.fill_bytes(&mut first).unwrap();
        OsRandom.fill_bytes(&mut second).unwrap();
        assert_ne!(first, second);
    }
}
