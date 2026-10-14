//! The hardware seam: what a transport (`PCIe`, or a simulator) must provide.
//!
//! **Unstable.** The shared `hal` crate's seam, re-exported unchanged, so custom
//! buses, including the board simulator, can drive a [`Device`](crate::Device). The `LSDr`
//! uses gateware register 0, SPI buses 0 to 2 (the ADC sockets and the LMK04832) and I2C
//! bus 0 of core 0 (TPS6594, Si549); it streams nothing yet.

pub use hal::{Bus, BusError, I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};
