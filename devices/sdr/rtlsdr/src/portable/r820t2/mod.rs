//! The Rafael R820T2 tuner, from its register description only.
//!
//! Source: Rafael Microelectronics, *R820T2 Register Description* (VER1.3, 2014-10-14; 11
//! pages; SHA-256 `52072cb9…695e68e`), cited by page and table in `PROVENANCE.md`. What
//! the document defines is here: the I2C framing ([`registers`]), the register fields
//! ([`field`]), their enumerated settings ([`setting`]), the PLL divider number ([`pll`])
//! and the IF VGA gain steps ([`gain`]). What it does not define (the VCO range and so the
//! LO plan, the LNA and mixer gains in dB, filter corners in hertz, band edges, power-on
//! defaults, the R828D) is a GAP row, not code.

pub mod field;
pub mod gain;
pub mod pll;
pub mod registers;
pub mod setting;

pub use field::Field;
pub use registers::{PageWrite, RegisterError, Registers};
pub use setting::FieldValue;
