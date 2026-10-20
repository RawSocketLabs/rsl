//! Calibration mathematics libusdr's SDR drivers share, ported as scalar code: integer
//! minimum searches ([`search`], `cal/opt_func.c`) and sinusoid fitting over captured IQ
//! samples ([`fit`], `cal/cal_filt.c`). The uSDR's LO and IQ calibration uses both; the LMS7
//! family's TX quality check fits its captured tone.
//!
//! The arithmetic follows the C operation by operation, including its single/double
//! precision promotions and integer truncations, so results match libusdr's bit for bit on
//! the same libm with C built without FMA contraction (the uSDR oracle's `cal_parity` test
//! checks this; see [`fit`] for the conditions). That also keeps the C's quirks; each is
//! documented where it lives. Nothing here touches hardware.
//!
//! **Unstable; unpublished.**
#![forbid(unsafe_code)]

pub mod fit;
pub mod search;
