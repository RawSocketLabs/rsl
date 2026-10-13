//! LMS6002D register values and operations without I/O (register meanings from libusdr's
//! `hw/lms6002d/lms6002d.yaml`, values from `hw/lms6002d/lms6002d.c`): one module per
//! datasheet block, each with its own `Reg`, the SPI word ([`spi`]), the cached chip state
//! and its straight-line operations ([`chip`]), the synthesizer tuning ([`tune`]) and the
//! DC calibrations ([`calibration`]).

mod afe;
pub(crate) mod calibration;
pub(crate) mod chip;
mod dc_cal;
mod lpf;
mod pll;
pub(crate) mod rx_fe;
mod rx_vga2;
pub(crate) mod spi;
mod top;
pub(crate) mod tune;
mod tx_rf;

pub(crate) use calibration::RxDcCalibration;
pub(crate) use chip::Lms6002d;
pub(crate) use pll::Pll;
pub(crate) use rx_fe::{Lna, LnaGain};
pub(crate) use tune::{CapacitorWindow, PllTune, TuneFault};
pub(crate) use tx_rf::PowerAmp;
