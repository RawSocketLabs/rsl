//! The hardware seam: what a transport (USB, `PCIe`, or a simulator) must provide.
//!
//! **Unstable.** The shared `rsl-lowlevel` crate's seam, re-exported unchanged, so custom
//! buses, including the board simulator, can drive a [`Device`](crate::Device). The XSDR
//! uses I2C core 0 (bus 0 for its PMIC, bus 1 for the front-end connector) and SPI bus 0
//! for the LMS7002M; it streams nothing yet.

pub use rsl_lowlevel::{Bus, BusError, I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};
