//! Platform-independent RTL-SDR code: the evidence placeholders, the RTL2832U facts (all
//! pending captures so far) and the R820T2 register maths. Meant to stay free of I/O and
//! of std outside tests, but no `no_std` witness checks that yet; keep imports within this
//! tree.

pub(crate) mod pending;
pub mod r820t2;
pub mod rtl2832u;
