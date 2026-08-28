//! The SHA-2 family of cryptographic digest algorithms.
//!
//! Family members live in separate modules because their word sizes, constants, block sizes,
//! and output rules differ. Shared code should be introduced only after complete reference
//! implementations demonstrate that sharing makes the specification easier to follow.
//!
//! [`sha256`] and [`sha512`] are implemented independently. Each module starts with the
//! algorithm's purpose and works down through padding, scheduling, compression, and output
//! serialization. The similar-looking implementations intentionally retain their different word
//! widths, rotation constants, block sizes, length fields, and round counts.

pub mod sha256;
pub mod sha512;
