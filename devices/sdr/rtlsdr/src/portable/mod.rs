//! Platform-independent RTL-SDR code: the evidence placeholders, the RTL2832U facts (all
//! pending captures so far) and the R820T2 register maths. Core only: no std, no
//! allocation; keep imports within this tree.

pub(crate) mod pending;
pub mod r820t2;
pub mod rtl2832u;
