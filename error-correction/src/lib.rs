//! Accuracy-first forward-error-correction contracts.
//!
//! A successful decode always carries a report distinguishing clean input from corrected input.
//! Each concrete decoder owns its uncorrectable-input error and any richer diagnostic evidence.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

/// What a successful decoder did to recover its output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Correction {
    /// The input was already a valid codeword.
    #[default]
    Clean,
    /// The decoder corrected one or more symbols.
    Corrected {
        /// Number of corrected symbols in the code's native symbol width.
        symbols: usize,
    },
}

/// Recovered data together with its correction status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    bytes: Vec<u8>,
    correction: Correction,
}

impl Decoded {
    /// Construct a successful decoding result.
    #[must_use]
    pub fn new(bytes: Vec<u8>, correction: Correction) -> Self {
        Self { bytes, correction }
    }

    /// Borrow the recovered information bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Return the correction status.
    #[must_use]
    pub fn correction(&self) -> Correction {
        self.correction
    }

    /// Consume the result into recovered bytes and correction status.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, Correction) {
        (self.bytes, self.correction)
    }
}

/// A context that adds redundancy to information bytes.
pub trait Encoder {
    /// The algorithm-specific failure type.
    type Error;

    /// Encode one information block into a complete codeword.
    ///
    /// # Errors
    ///
    /// Returns the algorithm's error when the information length or configuration cannot be
    /// represented by the selected code.
    fn encode(&mut self, information: &[u8]) -> core::result::Result<Vec<u8>, Self::Error>;

    /// Reset any state carried between codewords.
    fn reset(&mut self);
}

/// A context that validates and, when possible, corrects a codeword.
pub trait Decoder {
    /// The algorithm-specific failure type, including uncorrectable input.
    type Error;

    /// Decode one complete codeword.
    ///
    /// # Errors
    ///
    /// Returns the algorithm's error when the codeword is malformed or uncorrectable.
    fn decode(&mut self, codeword: &[u8]) -> core::result::Result<Decoded, Self::Error>;

    /// Reset any state carried between codewords.
    fn reset(&mut self);
}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::vec;

    #[test]
    fn decoded_data_cannot_hide_that_correction_occurred() {
        let decoded = Decoded::new(vec![0x52, 0x53, 0x4c], Correction::Corrected { symbols: 2 });

        assert_eq!(decoded.bytes(), b"RSL");
        assert_eq!(decoded.correction(), Correction::Corrected { symbols: 2 });
        assert_eq!(
            decoded.into_parts(),
            (vec![0x52, 0x53, 0x4c], Correction::Corrected { symbols: 2 })
        );
    }
}
