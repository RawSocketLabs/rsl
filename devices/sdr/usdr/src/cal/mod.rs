//! Calibration mathematics shared by libusdr's calibrations, ported as scalar code:
//! integer minimum searches ([`search`], `cal/opt_func.c`) and sinusoid fitting over
//! captured IQ samples ([`fit`], `cal/cal_filt.c`).
//!
//! The arithmetic follows the C operation by operation, including its single/double
//! precision promotions and integer truncations, so results match libusdr's bit for bit on
//! the same libm with C built without FMA contraction (the oracle's `cal_parity` test
//! checks this; see [`fit`] for the conditions). That also keeps the C's quirks; each is
//! documented where it lives. Nothing here touches hardware.

pub mod fit;
pub mod search;
