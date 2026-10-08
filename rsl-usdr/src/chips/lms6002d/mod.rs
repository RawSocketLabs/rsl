//! LMS6002D RF transceiver.
//!
//! The chip's register map is split into blocks by address, as in the datasheet: one module
//! per block, each with its own `Reg` enum. Register meanings come from libusdr's
//! `hw/lms6002d/lms6002d.yaml` (vendored under `oracle/libusdr`), and the values written
//! come from `hw/lms6002d/lms6002d.c`.

mod afe;
mod lms6002d;
mod pll;
mod rx_fe;
mod rx_vga2;
mod spi;
mod top;
mod tx_rf;

pub(crate) use lms6002d::Lms6002d;
pub(crate) use rx_fe::Lna;
pub(crate) use tx_rf::PowerAmp;
