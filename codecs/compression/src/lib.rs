//! Accuracy-first compression and decompression contracts.
//!
//! Compression state is directional and may span protocol messages. The contracts therefore use
//! mutable contexts, explicit flush requests, caller-visible output, and exact progress reports.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

/// How a compression context should terminate the current update.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Flush {
    /// Retain all state and emit only output naturally made available by the input.
    #[default]
    None,
    /// Make all input supplied so far available to the decoder while retaining stream state.
    Sync,
    /// Finish the stream and emit its terminal representation.
    Finish,
}

/// Exact progress made by one compression or decompression update.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Number of input bytes consumed.
    pub consumed: usize,
    /// Number of bytes appended to the output.
    pub produced: usize,
}

/// A stateful compression context.
pub trait Compressor {
    /// The algorithm-specific failure type.
    type Error;

    /// Consume input and append compressed bytes to `output`.
    ///
    /// # Errors
    ///
    /// Returns the algorithm's error when input, state, dictionary, or output limits prevent the
    /// requested update.
    fn compress(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        flush: Flush,
    ) -> core::result::Result<Progress, Self::Error>;

    /// Reset dictionary, history, and stream position to the initial state.
    fn reset(&mut self);
}

/// A stateful decompression context.
pub trait Decompressor {
    /// The algorithm-specific failure type.
    type Error;

    /// Consume compressed input and append recovered bytes to `output`.
    ///
    /// # Errors
    ///
    /// Returns the algorithm's error for malformed, truncated, or otherwise undecodable input,
    /// or when an output limit is reached.
    fn decompress(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        flush: Flush,
    ) -> core::result::Result<Progress, Self::Error>;

    /// Reset dictionary, history, and stream position to the initial state.
    fn reset(&mut self);
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn progress_keeps_input_and_output_counts_distinct() {
        let progress = Progress {
            consumed: 100,
            produced: 12,
        };

        assert_eq!(progress.consumed, 100);
        assert_eq!(progress.produced, 12);
        assert_ne!(progress.consumed, progress.produced);
    }
}
