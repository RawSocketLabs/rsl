//! Pure-Rust driver for libusdr's `LSDr` (`m2_da09_4_ad45_2`), a receive-only ADC board.
//!
//! [`Device`] talks to the board through a [`lowlevel::Bus`], the register/SPI/I2C seam of
//! libusdr's `ls_op`, so its operations are checked against libusdr's on a simulated board
//! (`rsl-lsdr-oracle`).
//!
//! **First slice; unstable; unpublished.** Ported: open with libusdr's front-end probe
//! ([`Device::open`], with what it finds in [`Device::fitted`]; the probe is
//! `rsl-sdr-frontend`'s), ADC power ([`Device::set_power`]), the receive PGA gain
//! ([`Device::set_rx_pga_gain`]) and close ([`Device::close`]). Not yet ported: the
//! fe1005000 and fe4ch4007200 front-end bring-ups, the sample rate (Si549
//! frequency, FIR gearbox, LMK04832 layout, ADC normal mode, LVDS PHY), the NCO frequency,
//! RX streaming and the temperature sensor. See `PORTING.md` beside this crate.
//!
//! # The board
//!
//! An M.2 card: an FPGA bridges `PCIe` to one or two TI `ADS42LBx9` dual-channel ADCs (SPI
//! buses 0 and 1), clocked by an Si549 oscillator (I2C bus 0, 0x67) or, on dual-ADC fits,
//! an LMK04832 clock generator (SPI bus 2); a TPS6594 PMIC (I2C bus 0, 0x48) supplies them,
//! its LDO4 feeding the ADCs. It has no transmit path and no DAC, and libusdr declares none.
//! libusdr reads no hardware ID: open probes each socket instead.
#![forbid(unsafe_code)]

mod board;
mod device;
mod error;
mod frontend;
pub mod lowlevel;
mod portable;

pub use device::{Device, OpenOptions};
pub use error::{Access, Error};
pub use portable::pga::PgaGain;
pub use portable::probes::Fitted;
pub use rsl_sdr_frontend::Board as FrontEndBoard;
