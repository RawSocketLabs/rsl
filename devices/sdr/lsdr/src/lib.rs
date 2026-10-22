//! Pure-Rust driver for libusdr's `LSDr` (`m2_da09_4_ad45_2`), a receive-only ADC board.
//!
//! [`Device`] talks to the board through a [`lowlevel::Bus`], the register/SPI/I2C seam of
//! libusdr's `ls_op`, so its operations are checked against libusdr's on a simulated board
//! (`lsdr-oracle`).
//!
//! **Unstable; unpublished.** Ported: open with libusdr's front-end probe
//! ([`Device::open`], with what it finds in [`Device::fitted`] and
//! [`Device::front_end`]; the probe and the fe1005000 and fe4ch4007200 bring-ups are
//! `sdr-frontend`'s), ADC
//! power ([`Device::set_power`]), the sample rate ([`Device::set_sample_rate`]: Si549
//! frequency, FIR gearbox, LMK04832 layout, ADC normal mode, LVDS PHY), the receive
//! frequency with its NCO ([`Device::set_rx_frequency`]), the receive PGA gain
//! ([`Device::set_rx_pga_gain`]), the temperature ([`Device::temperature`]), the RX
//! stream's create as far as libusdr gets ([`Device::create_rx_stream`], which fails at the
//! pinned revision as libusdr's does), fe1005000's debug-command controls
//! ([`Device::configure_fe1005000`]) and close ([`Device::close`]). Not yet ported:
//! fe1005000's other device-level controls (gains with a front end, `/dm/sdr/0/fe/*`),
//! fe4ch4007200's controls, `/dm/stat`'s PHY sweep and the debug paths. See `PORTING.md`
//! beside this crate.
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

pub use device::{Device, OpenOptions, RxChannels};
pub use error::{Access, Error};
pub use portable::lmk04832::ClockPlanFailure;
pub use portable::pga::PgaGain;
pub use portable::probes::Fitted;
pub use sdr_frontend::{Board as FrontEndBoard, Fe1005000Setting, FrontEndKind};
