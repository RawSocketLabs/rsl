//! Register-level models of the chips on the LMS7 family boards. The LMS7002M model is
//! shared with the other LMS7002M boards' simulators, in `lms7002m-sim`; the front-end
//! boards' chips, and the I2C seam every chip here answers on, are `sdr-frontend-sim`'s.

mod dac80501;
mod lp8758;

pub(crate) use dac80501::Dac80501;
pub(crate) use lp8758::Lp8758;
pub(crate) use sdr_frontend_sim::chips::I2cChip;
