//! The hardware seam: what a transport (USB, `PCIe`, or a simulator) must provide.
//!
//! **Unstable.** The shared `hal` crate's seam, re-exported unchanged, so custom
//! buses, including the board simulator, can drive a [`Device`](crate::Device). `PE_SYNC`
//! uses two I²C cores (core 0: the LMK05318B on bus 0, the LMK1D1208Is on bus 1; core 1:
//! the LP87524 on bus 0) and SPI buses 2 to 5 for its clock chips; it streams nothing.

pub use hal::{Bus, BusError, I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};
