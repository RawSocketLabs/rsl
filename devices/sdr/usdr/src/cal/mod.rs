//! Calibration mathematics shared by libusdr's calibrations, ported as scalar code:
//! integer minimum searches ([`search`], `cal/opt_func.c`) and sinusoid fitting over
//! captured IQ samples ([`fit`], `cal/cal_filt.c`). Both are the shared `sdr-cal`'s
//! (`devices/sdr/cal`, 2026-10-19: the LMS7 family's TX quality check fits too),
//! re-exported here where the board and the oracle's `cal_parity` test find them.

pub use sdr_cal::{fit, search};
